//! Saved logins, encrypted at rest with Windows DPAPI (tied to the current Windows user).
//! Imports/exports the CSV format used by Chrome, Edge and Firefox.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use crate::{blocker, store::now_ms};

#[derive(Serialize, Deserialize, Clone)]
pub struct Login {
    pub id: u64,
    pub origin: String,
    pub username: String,
    pub password: String,
    pub created: u64,
}

impl Login {
    pub fn host(&self) -> String {
        blocker::host_of(&self.origin).unwrap_or_else(|| self.origin.to_lowercase())
    }
}

pub struct Vault {
    path: PathBuf,
    pub logins: Vec<Login>,
}

fn dpapi(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let ok = if protect {
            CryptProtectData(&input, windows::core::w!("Veil logins"), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        if ok.is_err() || output.pbData.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as _)));
        Some(bytes)
    }
}

impl Vault {
    pub fn open(profile_dir: &Path) -> Self {
        let path = profile_dir.join("passwords.dat");
        let logins = std::fs::read(&path)
            .ok()
            .and_then(|enc| dpapi(&enc, false))
            .and_then(|plain| serde_json::from_slice(&plain).ok())
            .unwrap_or_default();
        Self { path, logins }
    }

    pub fn save(&self) {
        let Ok(plain) = serde_json::to_vec(&self.logins) else { return };
        if let Some(enc) = dpapi(&plain, true) {
            if let Some(dir) = self.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&self.path, enc);
        }
    }

    pub fn add(&mut self, origin: &str, username: &str, password: &str) -> bool {
        let origin = origin.trim();
        if origin.is_empty() || password.is_empty() {
            return false;
        }
        let origin = if origin.contains("://") { origin.to_string() } else { format!("https://{origin}") };
        if let Some(l) = self.logins.iter_mut().find(|l| l.origin == origin && l.username == username) {
            l.password = password.into();
        } else {
            let id = now_ms() * 1000 + (self.logins.len() as u64 % 1000);
            self.logins.push(Login { id, origin, username: username.into(), password: password.into(), created: now_ms() });
        }
        true
    }

    pub fn remove(&mut self, id: u64) {
        self.logins.retain(|l| l.id != id);
    }

    /// Logins whose site matches `url`'s host (same host or a parent/child domain).
    pub fn matching(&self, url: &str) -> Vec<&Login> {
        let Some(host) = blocker::host_of(url) else { return Vec::new() };
        let strip = |h: &str| h.trim_start_matches("www.").to_string();
        let host = strip(&host);
        self.logins
            .iter()
            .filter(|l| {
                let lh = strip(&l.host());
                lh == host || host.ends_with(&format!(".{lh}")) || lh.ends_with(&format!(".{host}"))
            })
            .collect()
    }

    /// Imports a browser-exported CSV. Returns how many logins were added or updated.
    pub fn import_csv(&mut self, text: &str) -> Result<usize, String> {
        let rows = parse_csv(text.trim_start_matches('\u{feff}'));
        let header: Vec<String> = rows.first().ok_or("The file is empty")?.iter().map(|h| h.trim().to_lowercase()).collect();
        let col = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
        let url = col(&["url", "origin", "website", "login_uri", "hostname"]).ok_or("No 'url' column found")?;
        let pass = col(&["password", "login_password"]).ok_or("No 'password' column found")?;
        let user = col(&["username", "login", "login_username", "user", "email"]);
        let mut n = 0;
        for row in rows.iter().skip(1) {
            let get = |i: usize| row.get(i).map(String::as_str).unwrap_or("");
            if self.add(get(url), user.map(get).unwrap_or(""), get(pass)) {
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn export_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = String::from("name,url,username,password\n");
        for l in &self.logins {
            out.push_str(&format!("{},{},{},{}\n", q(&l.host()), q(&l.origin), q(&l.username), q(&l.password)));
        }
        out
    }
}

/// RFC 4180 CSV parser (quoted fields, escaped quotes, CRLF, newlines inside quotes).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, _) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
                row.clear();
            }
            (false, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// JavaScript that fills a login form on the current page.
pub fn fill_script(username: &str, password: &str) -> String {
    let u = serde_json::to_string(username).unwrap_or_default();
    let p = serde_json::to_string(password).unwrap_or_default();
    format!(
        r#"(() => {{
  const vis = (e) => e && !e.disabled && e.offsetParent !== null;
  const set = (e, v) => {{
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    setter.call(e, v);
    e.dispatchEvent(new Event('input', {{ bubbles: true }}));
    e.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }};
  const pw = [...document.querySelectorAll('input[type=password]')].find(vis);
  const scope = (pw && pw.form) || document;
  const users = [...scope.querySelectorAll('input[type=email],input[type=text],input[autocomplete=username],input:not([type])')].filter(vis);
  const user = pw ? users.filter(e => e.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING).pop() : users[0];
  if (user && {u}) set(user, {u});
  if (pw) set(pw, {p});
  (pw || user)?.focus();
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parsing() {
        let rows = parse_csv("name,url,username,password\r\n\"a, b\",https://x.com,\"me\",\"p\"\"w\"\nz,https://y.com,u,\"multi\nline\"\n");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["a, b", "https://x.com", "me", "p\"w"]);
        assert_eq!(rows[2][3], "multi\nline");
    }

    #[test]
    fn import_and_match() {
        let mut v = Vault { path: PathBuf::new(), logins: Vec::new() };
        let n = v.import_csv("url,username,password\nhttps://accounts.example.com/login,bob,secret\n").unwrap();
        assert_eq!(n, 1);
        assert_eq!(v.matching("https://example.com/").len(), 1);
        assert_eq!(v.matching("https://other.com/").len(), 0);
        assert!(v.import_csv("foo,bar\n1,2\n").is_err());
    }
}
