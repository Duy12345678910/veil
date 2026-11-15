//! Per-profile persistent data: settings, history, bookmarks, downloads and the profile list.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

const MAX_HISTORY: usize = 5000;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// %APPDATA%\Veil — JSON data (settings, history, bookmarks, passwords).
pub fn data_root() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Veil")
}

/// %LOCALAPPDATA%\Veil — WebView2 engine data (cookies, cache, extensions).
pub fn engine_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Veil")
}

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save<T: Serialize>(path: &Path, value: &T) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(value) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Shortcut {
    pub title: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub search_engine: String,
    pub theme: String,
    pub block_trackers: bool,
    pub https_only: bool,
    pub strip_tracking: bool,
    pub send_gpc: bool,
    pub block_third_party_cookies: bool,
    pub strict_tracking_prevention: bool,
    pub save_history: bool,
    pub clear_on_exit: bool,
    pub show_bookmarks_bar: bool,
    pub shortcuts: Vec<Shortcut>,
}

impl Default for Settings {
    fn default() -> Self {
        let sc = |t: &str, u: &str| Shortcut { title: t.into(), url: u.into() };
        Self {
            search_engine: "duckduckgo".into(),
            theme: "claude-dark".into(),
            block_trackers: true,
            https_only: true,
            strip_tracking: true,
            send_gpc: true,
            block_third_party_cookies: true,
            strict_tracking_prevention: true,
            save_history: true,
            clear_on_exit: false,
            show_bookmarks_bar: true,
            shortcuts: vec![
                sc("Claude", "https://claude.ai/"),
                sc("DuckDuckGo", "https://duckduckgo.com/"),
                sc("Wikipedia", "https://www.wikipedia.org/"),
                sc("GitHub", "https://github.com/"),
                sc("YouTube", "https://www.youtube.com/"),
                sc("Reddit", "https://www.reddit.com/"),
                sc("Proton Mail", "https://mail.proton.me/"),
                sc("Rust", "https://www.rust-lang.org/"),
            ],
        }
    }
}

pub const SEARCH_ENGINES: &[(&str, &str, &str)] = &[
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q="),
    ("brave", "Brave Search", "https://search.brave.com/search?q="),
    ("startpage", "Startpage", "https://www.startpage.com/do/search?q="),
    ("mojeek", "Mojeek", "https://www.mojeek.com/search?q="),
    ("google", "Google", "https://www.google.com/search?q="),
    ("bing", "Bing", "https://www.bing.com/search?q="),
];

impl Settings {
    pub fn search_prefix(&self) -> &'static str {
        SEARCH_ENGINES.iter().find(|e| e.0 == self.search_engine).unwrap_or(&SEARCH_ENGINES[0]).2
    }
    pub fn search_name(&self) -> &'static str {
        SEARCH_ENGINES.iter().find(|e| e.0 == self.search_engine).unwrap_or(&SEARCH_ENGINES[0]).1
    }
}

/// Privacy switches read live by request/navigation hooks.
#[derive(Default)]
pub struct Flags {
    pub block_trackers: AtomicBool,
    pub https_only: AtomicBool,
    pub strip_tracking: AtomicBool,
    pub send_gpc: AtomicBool,
}

impl Flags {
    pub fn sync(&self, s: &Settings) {
        self.block_trackers.store(s.block_trackers, Ordering::Relaxed);
        self.https_only.store(s.https_only, Ordering::Relaxed);
        self.strip_tracking.store(s.strip_tracking, Ordering::Relaxed);
        self.send_gpc.store(s.send_gpc, Ordering::Relaxed);
    }
    pub fn get(flag: &AtomicBool) -> bool {
        flag.load(Ordering::Relaxed)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub time: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Bookmark {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub added: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub path: String,
    pub state: String, // "active" | "done" | "failed"
    pub time: u64,
}

pub struct Store {
    dir: PathBuf,
    pub settings: Settings,
    pub history: Vec<HistoryEntry>,
    pub bookmarks: Vec<Bookmark>,
    pub downloads: Vec<Download>,
}

impl Store {
    pub fn open(dir: PathBuf) -> Self {
        Self {
            settings: load(&dir.join("settings.json")),
            history: load(&dir.join("history.json")),
            bookmarks: load(&dir.join("bookmarks.json")),
            downloads: load(&dir.join("downloads.json")),
            dir,
        }
    }

    pub fn save_settings(&self) {
        save(&self.dir.join("settings.json"), &self.settings);
    }
    pub fn save_history(&self) {
        save(&self.dir.join("history.json"), &self.history);
    }
    pub fn save_bookmarks(&self) {
        save(&self.dir.join("bookmarks.json"), &self.bookmarks);
    }
    pub fn save_downloads(&self) {
        save(&self.dir.join("downloads.json"), &self.downloads);
    }

    pub fn record_visit(&mut self, url: &str, title: &str) {
        if !self.settings.save_history || !(url.starts_with("http://") || url.starts_with("https://")) {
            return;
        }
        if let Some(last) = self.history.last_mut() {
            if last.url == url {
                last.time = now_ms();
                if !title.is_empty() {
                    last.title = title.into();
                }
                self.save_history();
                return;
            }
        }
        self.history.push(HistoryEntry { url: url.into(), title: title.into(), time: now_ms() });
        if self.history.len() > MAX_HISTORY {
            let extra = self.history.len() - MAX_HISTORY;
            self.history.drain(..extra);
        }
        self.save_history();
    }

    pub fn update_title(&mut self, url: &str, title: &str) {
        if let Some(e) = self.history.iter_mut().rev().take(20).find(|e| e.url == url) {
            if e.title != title {
                e.title = title.into();
                self.save_history();
            }
        }
    }

    pub fn bookmark_for(&self, url: &str) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|b| b.url == url)
    }

    pub fn add_bookmark(&mut self, title: &str, url: &str) -> u64 {
        if let Some(b) = self.bookmark_for(url) {
            return b.id;
        }
        let id = now_ms() + self.bookmarks.len() as u64;
        self.bookmarks.push(Bookmark { id, title: title.into(), url: url.into(), added: now_ms() });
        self.save_bookmarks();
        id
    }

    /// Address-bar suggestions from bookmarks and history.
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<(String, String, &'static str)> {
        let q = query.to_lowercase();
        let mut out: Vec<(String, String, &'static str)> = Vec::new();
        let hit = |title: &str, url: &str| title.to_lowercase().contains(&q) || url.to_lowercase().contains(&q);
        for b in self.bookmarks.iter().filter(|b| hit(&b.title, &b.url)) {
            out.push((b.title.clone(), b.url.clone(), "bookmark"));
        }
        for h in self.history.iter().rev().filter(|h| hit(&h.title, &h.url)) {
            if out.len() >= limit {
                break;
            }
            if !out.iter().any(|o| o.1 == h.url) {
                out.push((h.title.clone(), h.url.clone(), "history"));
            }
        }
        out.truncate(limit);
        out
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub slug: String,
    pub color: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ProfileList {
    pub profiles: Vec<ProfileInfo>,
}

const PROFILE_COLORS: &[&str] = &["#d97757", "#6a9bd8", "#59b386", "#c27bd6", "#d6b45a", "#d66a8a"];

impl ProfileList {
    pub fn load() -> Self {
        let mut list: ProfileList = load(&data_root().join("profiles.json"));
        if list.profiles.is_empty() {
            list.profiles.push(ProfileInfo { name: "Default".into(), slug: "default".into(), color: PROFILE_COLORS[0].into() });
            list.save();
        }
        list
    }

    pub fn save(&self) {
        save(&data_root().join("profiles.json"), self);
    }

    pub fn create(&mut self, name: &str) -> Option<ProfileInfo> {
        let name = name.trim();
        if name.is_empty() || name.len() > 40 {
            return None;
        }
        let mut slug: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect();
        if slug.trim_matches('-').is_empty() {
            slug = "profile".into();
        }
        let base = slug.clone();
        let mut n = 2;
        while self.profiles.iter().any(|p| p.slug == slug) {
            slug = format!("{base}-{n}");
            n += 1;
        }
        let color = PROFILE_COLORS[self.profiles.len() % PROFILE_COLORS.len()].to_string();
        let info = ProfileInfo { name: name.into(), slug, color };
        self.profiles.push(info.clone());
        self.save();
        Some(info)
    }

    pub fn remove(&mut self, slug: &str) -> bool {
        if slug == "default" {
            return false;
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.slug != slug);
        let removed = self.profiles.len() != before;
        if removed {
            self.save();
            let _ = std::fs::remove_dir_all(data_root().join("profiles").join(slug));
            let _ = std::fs::remove_dir_all(engine_root().join("profiles").join(slug));
        }
        removed
    }
}

pub fn profile_data_dir(slug: &str) -> PathBuf {
    data_root().join("profiles").join(slug)
}

pub fn profile_engine_dir(slug: &str) -> PathBuf {
    engine_root().join("profiles").join(slug)
}
