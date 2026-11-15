//! Messages from Veil's own UI (toolbar, popup, internal pages) and how they are handled.

use serde::Deserialize;
use serde_json::{json, Value};
use tao::event_loop::EventLoopWindowTarget;
use tao::window::WindowId;

use crate::app::App;
use crate::keys::{Action, SHORTCUTS};
use crate::store::{Settings, Shortcut, SEARCH_ENGINES};
use crate::views::{Source, UserEvent};
use crate::window::{NAV_H, TABS_H};
use crate::{engine, passwords};

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Cmd {
    Ready,
    Act { action: Action },
    Navigate { url: String, #[serde(default)] new_tab: bool },
    SwitchTab { id: u32 },
    CloseTab { id: u32 },
    Popup { mode: String, #[serde(default)] x: f64, #[serde(default)] w: f64 },
    PopupClose { #[serde(default)] mode: Option<String> },
    PopupSize { h: f64 },
    UrlInput { text: String },
    SuggestShow { items: Value, index: i64, x: f64, w: f64 },
    FindText { text: String, #[serde(default)] backwards: bool },
    FindClose,
    Get { what: String },
    SetSetting { key: String, value: Value },
    DeleteHistory { time: u64 },
    ClearHistory,
    AddBookmark { title: String, url: String },
    EditBookmark { id: u64, title: String, url: String },
    DeleteBookmark { id: u64 },
    ImportBookmarks,
    ExportBookmarks,
    DeleteDownload { id: u64 },
    ClearDownloads,
    OpenDownload { id: u64, #[serde(default)] folder: bool },
    AddPassword { origin: String, username: String, password: String },
    DeletePassword { id: u64 },
    ImportPasswords,
    ExportPasswords,
    FillLogin { id: u64 },
    AddExtension,
    RemoveExtension { id: String },
    ToggleExtension { id: String, enabled: bool },
    CreateProfile { name: String },
    DeleteProfile { slug: String },
    OpenProfile { slug: String, #[serde(default)] private: bool },
    ClearData { history: bool, cookies: bool, cache: bool, downloads: bool },
    Cookies { #[serde(default)] delete: Option<String> },
    SetShortcuts { list: Vec<Shortcut> },
}

fn themes() -> Value {
    json!([["claude-dark", "Claude Dark"], ["claude-light", "Claude Light"], ["ember", "Ember"], ["midnight", "Midnight"]])
}

fn bookmarks_html(items: &[crate::store::Bookmark]) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;");
    let mut out = String::from("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n");
    for b in items {
        out.push_str(&format!("    <DT><A HREF=\"{}\" ADD_DATE=\"{}\">{}</A>\n", esc(&b.url), b.added / 1000, esc(&b.title)));
    }
    out.push_str("</DL><p>\n");
    out
}

fn parse_bookmarks_html(html: &str) -> Vec<(String, String)> {
    let unesc = |s: &str| s.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&#39;", "'").replace("&amp;", "&");
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("href=\"") {
        let start = pos + i + 6;
        let Some(end) = lower[start..].find('"').map(|e| start + e) else { break };
        let url = unesc(&html[start..end]);
        let title_start = lower[end..].find('>').map(|e| end + e + 1).unwrap_or(end);
        let title_end = lower[title_start..].find("</a>").map(|e| title_start + e).unwrap_or(title_start);
        if url.starts_with("http") {
            out.push((unesc(html[title_start..title_end].trim()), url));
        }
        pos = title_end.max(end);
    }
    out
}

impl App {
    pub fn handle_ipc(&mut self, wid: WindowId, src: Source, body: &str, target: &EventLoopWindowTarget<UserEvent>) -> bool {
        let cmd = match serde_json::from_str::<Cmd>(body) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("veil: bad message {body}: {e}");
                return true;
            }
        };
        self.handle_cmd(wid, src, cmd, target)
    }

    fn src_view(&self, wid: WindowId, src: Source) -> Option<&wry::WebView> {
        let w = self.windows.get(&wid)?;
        match src {
            Source::Tab(id) => w.tab(id).map(|t| &t.view),
            Source::Toolbar => Some(&w.toolbar),
            Source::Popup => Some(&w.popup),
        }
    }

    fn async_reply(&self, wid: WindowId, src: Source, what: &str) -> engine::Done {
        let (proxy, what) = (self.proxy.clone(), what.to_string());
        Box::new(move |data| {
            let _ = proxy.send_event(UserEvent::Reply { wid, src, what, data });
        })
    }

    pub fn send_data(&mut self, wid: WindowId, src: Source, what: &str) {
        let Some(w) = self.windows.get(&wid) else { return };
        let private = w.private;
        let Some(rt) = self.prof(wid) else { return };
        let data = match what {
            "settings" => json!({
                "settings": rt.store.settings, "engines": SEARCH_ENGINES.iter().map(|e| [e.0, e.1]).collect::<Vec<_>>(),
                "themes": themes(), "private": private, "profile": rt.info.name,
            }),
            "history" => json!(rt.store.history.iter().rev().take(3000).collect::<Vec<_>>()),
            "bookmarks" => json!(rt.store.bookmarks),
            "downloads" => json!(rt.store.downloads.iter().rev().collect::<Vec<_>>()),
            "passwords" => json!(rt.vault.logins),
            "profiles" => json!({ "list": self.list.profiles, "current": rt.info.slug }),
            "help" => json!(SHORTCUTS),
            "newtab" => json!({
                "shortcuts": rt.store.settings.shortcuts, "search": rt.store.settings.search_name(),
                "private": private, "profile": rt.info.name, "color": rt.info.color,
            }),
            "extensions" | "cookies" => {
                let done = self.async_reply(wid, src, what);
                if let Some(view) = self.src_view(wid, src) {
                    if what == "extensions" {
                        engine::extensions(view, None, done);
                    } else {
                        engine::cookies(view, None, done);
                    }
                }
                return;
            }
            _ => return,
        };
        self.reply(wid, src, what, &data);
    }

    fn pick_file(&self, wid: WindowId, name: &str, ext: &[&str]) -> Option<std::path::PathBuf> {
        let w = self.windows.get(&wid)?;
        rfd::FileDialog::new().add_filter(name, ext).set_parent(&w.window).pick_file()
    }

    fn save_file(&self, wid: WindowId, name: &str, ext: &str, default: &str) -> Option<std::path::PathBuf> {
        let w = self.windows.get(&wid)?;
        rfd::FileDialog::new().add_filter(name, &[ext]).set_file_name(default).set_parent(&w.window).save_file()
    }

    fn toast(&self, wid: WindowId, src: Source, msg: &str) {
        self.reply(wid, src, "toast", &json!(msg));
    }

    fn handle_cmd(&mut self, wid: WindowId, src: Source, cmd: Cmd, target: &EventLoopWindowTarget<UserEvent>) -> bool {
        let Some(w) = self.windows.get_mut(&wid) else { return true };
        let active = w.active;
        match cmd {
            Cmd::Ready => match src {
                Source::Toolbar => self.push_state(wid),
                _ => {}
            },
            Cmd::Act { action } => return self.action(wid, action, target),
            Cmd::Navigate { url, new_tab } => {
                w.hide_popup();
                let tab = match src {
                    Source::Tab(id) if !new_tab => id,
                    _ => active,
                };
                if new_tab || w.tabs.is_empty() {
                    self.open_tab(wid, &url);
                } else {
                    self.navigate(wid, tab, &url);
                }
            }
            Cmd::SwitchTab { id } => {
                w.hide_popup();
                w.show_tab(id);
                self.push_state(wid);
            }
            Cmd::CloseTab { id } => {
                if !w.remove_tab(id) {
                    return self.close_window(wid);
                }
                self.push_state(wid);
            }
            Cmd::Popup { mode, x, w: width } => match mode.as_str() {
                "menu" => self.show_menu(wid),
                "bookmark" => self.bookmark_current(wid),
                "logins" => {
                    let url = w.active_tab().map(|t| t.url.clone()).unwrap_or_default();
                    let Some(rt) = self.profiles.get(&w.profile) else { return true };
                    let list: Vec<_> = rt.vault.matching(&url).iter().map(|l| json!({ "id": l.id, "username": l.username, "origin": l.origin })).collect();
                    let h = 64.0 + 44.0 * list.len().min(6) as f64;
                    w.show_popup("logins", &json!(list), x - 150.0, TABS_H + NAV_H - 4.0, width.max(320.0), h, true);
                }
                _ => {}
            },
            Cmd::PopupClose { mode } => {
                if mode.is_none() || mode == w.popup_mode {
                    w.hide_popup();
                }
            }
            Cmd::PopupSize { h } => w.fit_popup(h),
            Cmd::UrlInput { text } => {
                let Some(rt) = self.profiles.get(&w.profile) else { return true };
                let items: Vec<_> = if text.trim().is_empty() {
                    Vec::new()
                } else {
                    rt.store.suggest(text.trim(), 7).into_iter().map(|(t, u, k)| json!({ "title": t, "url": u, "kind": k })).collect()
                };
                let js = format!("window.veil && window.veil.suggestions({}, {})", json!(items), json!(rt.store.settings.search_name()));
                w.eval_toolbar(&js);
            }
            Cmd::SuggestShow { items, index, x, w: width } => {
                let n = items.as_array().map(|a| a.len()).unwrap_or(0);
                if n == 0 {
                    if w.popup_mode.as_deref() == Some("suggest") {
                        w.hide_popup();
                    }
                } else {
                    let data = json!({ "items": items, "index": index });
                    w.show_popup("suggest", &data, x, TABS_H + NAV_H - 6.0, width, 12.0 + 40.0 * n as f64, false);
                }
            }
            Cmd::FindText { text, backwards } => {
                if let Some(v) = w.active_web_view() {
                    let js = format!("window.find({}, false, {backwards}, true)", json!(text));
                    let _ = v.evaluate_script(&js);
                }
            }
            Cmd::FindClose => {
                w.hide_popup();
                if let Some(v) = w.active_web_view() {
                    let _ = v.evaluate_script("getSelection().removeAllRanges()");
                    let _ = v.focus();
                }
            }
            Cmd::Get { what } => self.send_data(wid, src, &what),
            Cmd::SetSetting { key, value } => {
                let slug = w.profile.clone();
                let Some(rt) = self.profiles.get_mut(&slug) else { return true };
                let mut all = serde_json::to_value(&rt.store.settings).unwrap_or_default();
                all[key.as_str()] = value;
                if let Ok(s) = serde_json::from_value::<Settings>(all) {
                    rt.store.settings = s;
                    rt.store.save_settings();
                    rt.flags.sync(&rt.store.settings);
                }
                self.refresh_profile_windows(&slug);
                self.send_data(wid, src, "settings");
            }
            Cmd::DeleteHistory { time } => {
                if let Some(rt) = self.prof_mut(wid) {
                    rt.store.history.retain(|h| h.time != time);
                    rt.store.save_history();
                }
                self.send_data(wid, src, "history");
            }
            Cmd::ClearHistory => {
                if let Some(rt) = self.prof_mut(wid) {
                    rt.store.history.clear();
                    rt.store.save_history();
                }
                self.send_data(wid, src, "history");
            }
            Cmd::AddBookmark { title, url } => {
                if let Some(rt) = self.prof_mut(wid) {
                    let url = if url.contains("://") { url } else { format!("https://{url}") };
                    rt.store.add_bookmark(if title.is_empty() { &url } else { &title }, &url);
                }
                self.after_bookmarks(wid, src);
            }
            Cmd::EditBookmark { id, title, url } => {
                if let Some(rt) = self.prof_mut(wid) {
                    if let Some(b) = rt.store.bookmarks.iter_mut().find(|b| b.id == id) {
                        b.title = title;
                        b.url = url;
                    }
                    rt.store.save_bookmarks();
                }
                self.after_bookmarks(wid, src);
            }
            Cmd::DeleteBookmark { id } => {
                if let Some(rt) = self.prof_mut(wid) {
                    rt.store.bookmarks.retain(|b| b.id != id);
                    rt.store.save_bookmarks();
                }
                if src == Source::Popup {
                    if let Some(w) = self.windows.get_mut(&wid) {
                        w.hide_popup();
                    }
                }
                self.after_bookmarks(wid, src);
            }
            Cmd::ImportBookmarks => {
                let Some(path) = self.pick_file(wid, "Bookmarks HTML", &["html", "htm"]) else { return true };
                let html = std::fs::read_to_string(&path).unwrap_or_default();
                let items = parse_bookmarks_html(&html);
                let n = items.len();
                if let Some(rt) = self.prof_mut(wid) {
                    for (t, u) in items {
                        rt.store.add_bookmark(&t, &u);
                    }
                }
                self.toast(wid, src, &format!("Imported {n} bookmarks"));
                self.after_bookmarks(wid, src);
            }
            Cmd::ExportBookmarks => {
                let Some(path) = self.save_file(wid, "Bookmarks HTML", "html", "veil-bookmarks.html") else { return true };
                let html = self.prof(wid).map(|rt| bookmarks_html(&rt.store.bookmarks)).unwrap_or_default();
                let ok = std::fs::write(&path, html).is_ok();
                self.toast(wid, src, if ok { "Bookmarks exported" } else { "Export failed" });
            }
            Cmd::DeleteDownload { id } => {
                if let Some(rt) = self.prof_mut(wid) {
                    rt.store.downloads.retain(|d| d.id != id);
                    rt.store.save_downloads();
                }
                self.send_data(wid, src, "downloads");
            }
            Cmd::ClearDownloads => {
                if let Some(rt) = self.prof_mut(wid) {
                    rt.store.downloads.retain(|d| d.state == "active");
                    rt.store.save_downloads();
                }
                self.send_data(wid, src, "downloads");
            }
            Cmd::OpenDownload { id, folder } => {
                if let Some(d) = self.prof(wid).and_then(|rt| rt.store.downloads.iter().find(|d| d.id == id)) {
                    let _ = if folder {
                        std::process::Command::new("explorer").arg(format!("/select,{}", d.path)).spawn()
                    } else {
                        std::process::Command::new("explorer").arg(&d.path).spawn()
                    };
                }
            }
            Cmd::AddPassword { origin, username, password } => {
                if let Some(rt) = self.prof_mut(wid) {
                    if rt.vault.add(&origin, &username, &password) {
                        rt.vault.save();
                    }
                }
                self.send_data(wid, src, "passwords");
                self.push_state(wid);
            }
            Cmd::DeletePassword { id } => {
                if let Some(rt) = self.prof_mut(wid) {
                    rt.vault.remove(id);
                    rt.vault.save();
                }
                self.send_data(wid, src, "passwords");
                self.push_state(wid);
            }
            Cmd::ImportPasswords => {
                let Some(path) = self.pick_file(wid, "Passwords CSV", &["csv"]) else { return true };
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let msg = match self.prof_mut(wid).map(|rt| {
                    let r = rt.vault.import_csv(&text);
                    rt.vault.save();
                    r
                }) {
                    Some(Ok(n)) => format!("Imported {n} passwords. You can now delete the CSV file."),
                    Some(Err(e)) => format!("Import failed: {e}"),
                    None => return true,
                };
                self.toast(wid, src, &msg);
                self.send_data(wid, src, "passwords");
                self.push_state(wid);
            }
            Cmd::ExportPasswords => {
                let Some(path) = self.save_file(wid, "Passwords CSV", "csv", "veil-passwords.csv") else { return true };
                let csv = self.prof(wid).map(|rt| rt.vault.export_csv()).unwrap_or_default();
                let ok = std::fs::write(&path, csv).is_ok();
                self.toast(wid, src, if ok { "Exported. The file is NOT encrypted — keep it safe." } else { "Export failed" });
            }
            Cmd::FillLogin { id } => {
                w.hide_popup();
                let Some(rt) = self.prof(wid) else { return true };
                if let (Some(l), Some(v)) = (rt.vault.logins.iter().find(|l| l.id == id), self.windows.get(&wid).and_then(|w| w.active_web_view())) {
                    if !rt.vault.matching(&self.windows[&wid].active_tab().map(|t| t.url.clone()).unwrap_or_default()).iter().any(|m| m.id == id) {
                        return true; // never fill a login into a site it doesn't belong to
                    }
                    let _ = v.evaluate_script(&passwords::fill_script(&l.username, &l.password));
                    let _ = v.focus();
                }
            }
            Cmd::AddExtension => {
                let folder = self.windows.get(&wid).and_then(|w| rfd::FileDialog::new().set_parent(&w.window).set_title("Select an unpacked extension folder").pick_folder());
                if let (Some(folder), Some(view)) = (folder, self.src_view(wid, src)) {
                    let done = self.async_reply(wid, src, "extension_added");
                    engine::add_extension(view, &folder.to_string_lossy(), done);
                }
            }
            Cmd::RemoveExtension { id } => {
                if let Some(view) = self.src_view(wid, src) {
                    engine::extensions(view, Some((id, None)), self.async_reply(wid, src, "extensions"));
                }
            }
            Cmd::ToggleExtension { id, enabled } => {
                if let Some(view) = self.src_view(wid, src) {
                    engine::extensions(view, Some((id, Some(enabled))), self.async_reply(wid, src, "extensions"));
                }
            }
            Cmd::CreateProfile { name } => {
                let created = self.list.create(&name);
                self.toast(wid, src, if created.is_some() { "Profile created" } else { "Enter a name (up to 40 characters)" });
                self.send_data(wid, src, "profiles");
            }
            Cmd::DeleteProfile { slug } => {
                let in_use = self.windows.values().any(|w| w.profile == slug);
                if in_use {
                    self.toast(wid, src, "Close this profile's windows first");
                } else if self.list.remove(&slug) {
                    self.profiles.remove(&slug);
                    self.toast(wid, src, "Profile deleted");
                }
                self.send_data(wid, src, "profiles");
            }
            Cmd::OpenProfile { slug, private } => self.open_window(target, &slug, private, None),
            Cmd::ClearData { history, cookies, cache, downloads } => {
                if let Some(rt) = self.prof_mut(wid) {
                    if history {
                        rt.store.history.clear();
                        rt.store.save_history();
                    }
                    if downloads {
                        rt.store.downloads.retain(|d| d.state == "active");
                        rt.store.save_downloads();
                    }
                }
                if cookies || cache {
                    if let Some(view) = self.src_view(wid, src) {
                        engine::clear_data(view, cookies, cache, self.async_reply(wid, src, "cleared"));
                    }
                } else {
                    self.reply(wid, src, "cleared", &json!({ "ok": true }));
                }
            }
            Cmd::Cookies { delete } => {
                if let Some(view) = self.src_view(wid, src) {
                    engine::cookies(view, delete, self.async_reply(wid, src, "cookies"));
                }
            }
            Cmd::SetShortcuts { list } => {
                let slug = w.profile.clone();
                if let Some(rt) = self.profiles.get_mut(&slug) {
                    rt.store.settings.shortcuts = list.into_iter().take(24).collect();
                    rt.store.save_settings();
                }
                self.send_data(wid, src, "newtab");
            }
        }
        true
    }

    fn after_bookmarks(&mut self, wid: WindowId, src: Source) {
        if matches!(src, Source::Tab(_)) {
            self.send_data(wid, src, "bookmarks");
        }
        let slug = self.windows.get(&wid).map(|w| w.profile.clone()).unwrap_or_default();
        let ids: Vec<WindowId> = self.windows.values().filter(|w| w.profile == slug).map(|w| w.id).collect();
        for id in ids {
            self.push_state(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_roundtrip() {
        let items = vec![crate::store::Bookmark { id: 1, title: "A & B".into(), url: "https://a.com/?x=1&y=2".into(), added: 0 }];
        let parsed = parse_bookmarks_html(&bookmarks_html(&items));
        assert_eq!(parsed, vec![("A & B".to_string(), "https://a.com/?x=1&y=2".to_string())]);
    }
}
