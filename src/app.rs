//! Application state: profiles, windows and tabs, plus event dispatch.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tao::dpi::LogicalSize;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Icon, WindowBuilder, WindowId};

use crate::passwords::Vault;
use crate::store::{self, Flags, ProfileInfo, ProfileList, Store};
use crate::views::{self, Ctx, Source, UserEvent};
use crate::window::{rect, BrowserWindow, Page, Tab, TABS_H};
use crate::{blocker, engine, ui};

pub struct ProfileRt {
    pub info: ProfileInfo,
    pub store: Store,
    pub vault: Vault,
    pub flags: Arc<Flags>,
    pub args: String,
}

pub struct App {
    pub proxy: EventLoopProxy<UserEvent>,
    pub windows: HashMap<WindowId, BrowserWindow>,
    pub profiles: HashMap<String, ProfileRt>,
    pub list: ProfileList,
    pub next_tab: u32,
    pub private_seq: u32,
    pub closing: HashMap<String, Vec<WindowId>>,
}

fn icon() -> Option<Icon> {
    Icon::from_rgba(include_bytes!(concat!(env!("OUT_DIR"), "/icon64.rgba")).to_vec(), 64, 64).ok()
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            proxy,
            windows: HashMap::new(),
            profiles: HashMap::new(),
            list: ProfileList::load(),
            next_tab: 1,
            private_seq: 0,
            closing: HashMap::new(),
        }
    }

    pub fn profile(&mut self, slug: &str) -> &mut ProfileRt {
        if !self.profiles.contains_key(slug) {
            let info = self.list.profiles.iter().find(|p| p.slug == slug).cloned().unwrap_or_else(|| self.list.profiles[0].clone());
            let dir = store::profile_data_dir(&info.slug);
            let store = Store::open(dir.clone());
            let flags = Arc::new(Flags::default());
            flags.sync(&store.settings);
            let args = views::browser_args(store.settings.block_third_party_cookies);
            let rt = ProfileRt { vault: Vault::open(&dir), store, flags, args, info };
            self.profiles.insert(slug.to_string(), rt);
        }
        self.profiles.get_mut(slug).unwrap()
    }

    pub fn prof(&self, wid: WindowId) -> Option<&ProfileRt> {
        self.windows.get(&wid).and_then(|w| self.profiles.get(&w.profile))
    }

    pub fn prof_mut(&mut self, wid: WindowId) -> Option<&mut ProfileRt> {
        let slug = self.windows.get(&wid)?.profile.clone();
        self.profiles.get_mut(&slug)
    }

    pub fn open_window(&mut self, target: &EventLoopWindowTarget<UserEvent>, slug: &str, private: bool, url: Option<String>) {
        let slug = self.profile(slug).info.slug.clone();
        let data_dir = if private {
            self.private_seq += 1;
            std::env::temp_dir().join(format!("veil-private-{}-{}", std::process::id(), self.private_seq))
        } else {
            store::profile_engine_dir(&slug)
        };
        let offset = 30.0 * (self.windows.len() % 8) as f64;
        let window = match WindowBuilder::new()
            .with_title("Veil")
            .with_window_icon(icon())
            .with_inner_size(LogicalSize::new(1280.0, 820.0))
            .with_min_inner_size(LogicalSize::new(520.0, 320.0))
            .with_position(tao::dpi::LogicalPosition::new(80.0 + offset, 60.0 + offset))
            .build(target)
        {
            Ok(w) => w,
            Err(e) => return eprintln!("veil: could not create window: {e}"),
        };
        let wid = window.id();
        let rt = &self.profiles[&slug];
        let theme = rt.store.settings.theme.clone();
        let bookmarks_bar = rt.store.settings.show_bookmarks_bar;
        let size: LogicalSize<f64> = window.inner_size().to_logical(window.scale_factor());
        let ctx = Ctx { window: &window, wid, proxy: &self.proxy, data_dir: &data_dir, private, args: &rt.args, profile: &slug };
        let built = views::ui_view(&ctx, ui::toolbar_html(&theme, private), rect(0.0, 0.0, size.width, TABS_H), Source::Toolbar, true)
            .and_then(|toolbar| {
                views::ui_view(&ctx, ui::popup_html(&theme, private), rect(0.0, 0.0, 10.0, 10.0), Source::Popup, false)
                    .map(|popup| (toolbar, popup))
            });
        let (toolbar, popup) = match built {
            Ok(v) => v,
            Err(e) => return eprintln!("veil: could not create toolbar: {e}"),
        };
        let bw = BrowserWindow {
            window, id: wid, profile: slug, private, data_dir, toolbar, popup,
            popup_mode: None, popup_geom: (0.0, 0.0, 0.0), tabs: Vec::new(), active: 0, closed: Vec::new(), bookmarks_bar, fullscreen: false,
        };
        bw.layout();
        self.windows.insert(wid, bw);
        self.open_tab(wid, &url.unwrap_or_else(|| Page::NewTab.url()));
    }

    pub fn search_prefix(&self, wid: WindowId) -> &'static str {
        self.prof(wid).map(|p| p.store.settings.search_prefix()).unwrap_or(store::SEARCH_ENGINES[0].2)
    }

    /// Builds the webview for `target` (a veil:// page or a web address).
    fn make_view(&self, wid: WindowId, tab: u32, target: &str) -> Option<(Option<Page>, wry::WebView, String, Arc<AtomicU32>)> {
        let w = self.windows.get(&wid)?;
        let rt = self.profiles.get(&w.profile)?;
        let ctx = Ctx { window: &w.window, wid, proxy: &self.proxy, data_dir: &w.data_dir, private: w.private, args: &rt.args, profile: &w.profile };
        let bounds = w.content_rect();
        let blocked = Arc::new(AtomicU32::new(0));
        let resolved = blocker::resolve_input(target, rt.store.settings.search_prefix());
        let result = if let Some(page) = Page::from_url(&resolved) {
            let html = ui::page_html(page, &rt.store.settings.theme, w.private);
            views::ui_view(&ctx, html, bounds, Source::Tab(tab), true).map(|v| (Some(page), v, page.url()))
        } else {
            let s = &rt.store.settings;
            views::web_view(&ctx, tab, &resolved, bounds, rt.flags.clone(), blocked.clone(), s.strict_tracking_prevention)
                .map(|v| (None, v, resolved.clone()))
        };
        match result {
            Ok((page, view, url)) => Some((page, view, url, blocked)),
            Err(e) => {
                eprintln!("veil: could not create tab: {e}");
                None
            }
        }
    }

    pub fn open_tab(&mut self, wid: WindowId, target: &str) {
        let id = self.next_tab;
        self.next_tab += 1;
        let Some((page, view, url, blocked)) = self.make_view(wid, id, target) else { return };
        let title = page.map(|p| p.title().to_string()).unwrap_or_else(|| "Loading…".into());
        let Some(w) = self.windows.get_mut(&wid) else { return };
        w.hide_popup();
        w.tabs.push(Tab { id, page, view, title, url, loading: page.is_none(), blocked, favicon: String::new() });
        w.show_tab(id);
        if page == Some(Page::NewTab) {
            let _ = w.toolbar.focus();
            w.eval_toolbar("window.veil && window.veil.focusUrl()");
        }
        self.push_state(wid);
    }

    /// Loads `input` in an existing tab, swapping between page and web views when needed.
    pub fn navigate(&mut self, wid: WindowId, tab: u32, input: &str) {
        let resolved = blocker::resolve_input(input, self.search_prefix(wid));
        let target_page = Page::from_url(&resolved);
        let Some(current) = self.windows.get(&wid).and_then(|w| w.tab(tab)) else { return };
        if current.page.is_none() && target_page.is_none() {
            let _ = current.view.load_url(&resolved);
            let _ = current.view.focus();
            return;
        }
        let Some((page, view, url, blocked)) = self.make_view(wid, tab, &resolved) else { return };
        let Some(w) = self.windows.get_mut(&wid) else { return };
        let active = w.active == tab;
        if let Some(t) = w.tab_mut(tab) {
            let _ = view.set_visible(active);
            t.view = view;
            t.page = page;
            t.url = url;
            t.blocked = blocked;
            t.favicon.clear();
            t.title = page.map(|p| p.title().to_string()).unwrap_or_else(|| "Loading…".into());
            t.loading = page.is_none();
            if active {
                let _ = t.view.focus();
            }
        }
        self.push_state(wid);
    }

    /// Opens an internal page, reusing an existing tab that already shows it.
    pub fn open_page(&mut self, wid: WindowId, page: Page) {
        let Some(w) = self.windows.get_mut(&wid) else { return };
        w.hide_popup();
        if let Some(id) = w.tabs.iter().find(|t| t.page == Some(page)).map(|t| t.id) {
            w.show_tab(id);
            self.push_state(wid);
        } else if w.active_tab().is_some_and(|t| t.page == Some(Page::NewTab)) {
            let active = w.active;
            self.navigate(wid, active, &page.url());
        } else {
            self.open_tab(wid, &page.url());
        }
    }

    pub fn push_state(&self, wid: WindowId) {
        let (Some(w), Some(rt)) = (self.windows.get(&wid), self.prof(wid)) else { return };
        let s = &rt.store.settings;
        let tabs: Vec<_> = w.tabs.iter().map(|t| serde_json::json!({
            "id": t.id, "title": t.title, "active": t.id == w.active, "favicon": t.favicon,
            "loading": t.loading, "internal": t.page.is_some(),
        })).collect();
        let (url, title, blocked, loading, web) = match w.active_tab() {
            Some(t) => {
                let shown = if t.page == Some(Page::NewTab) { String::new() } else { t.url.clone() };
                (shown, t.title.clone(), t.blocked.load(Ordering::Relaxed), t.loading, t.page.is_none())
            }
            None => (String::new(), String::new(), 0, false, false),
        };
        let bar: Vec<_> = rt.store.bookmarks.iter().take(40).map(|b| serde_json::json!({ "title": b.title, "url": b.url })).collect();
        let state = serde_json::json!({
            "tabs": tabs, "url": url, "title": title, "blocked": blocked, "loading": loading, "web": web,
            "bookmarked": web && rt.store.bookmark_for(&url).is_some(),
            "logins": if web { rt.vault.matching(&url).len() } else { 0 },
            "bookmarks": bar, "bookmarksBar": w.bookmarks_bar && !w.fullscreen, "private": w.private,
            "profile": { "name": rt.info.name, "color": rt.info.color },
            "downloads": rt.store.downloads.iter().filter(|d| d.state == "active").count(),
            "search": s.search_name(),
        });
        let suffix = if w.private { "Veil Private Browsing" } else { "Veil" };
        w.window.set_title(&if title.is_empty() { suffix.to_string() } else { format!("{title} — {suffix}") });
        w.eval_toolbar(&format!("window.veil && window.veil.update({state})"));
    }

    /// Sends data back to the webview that asked for it.
    pub fn reply(&self, wid: WindowId, src: Source, what: &str, data: &serde_json::Value) {
        let Some(w) = self.windows.get(&wid) else { return };
        let view = match src {
            Source::Toolbar => &w.toolbar,
            Source::Popup => &w.popup,
            Source::Tab(id) => match w.tab(id) {
                Some(t) => &t.view,
                None => return,
            },
        };
        let js = format!("window.veil && window.veil.recv({}, {})", serde_json::to_string(what).unwrap(), data);
        let _ = view.evaluate_script(&js);
    }

    /// Re-applies theme/bookmark-bar settings to every window of a profile.
    pub fn refresh_profile_windows(&mut self, slug: &str) {
        let Some(rt) = self.profiles.get(slug) else { return };
        let theme = serde_json::to_string(&rt.store.settings.theme).unwrap();
        let bar = rt.store.settings.show_bookmarks_bar;
        let js = format!("document.documentElement.dataset.theme = {theme}");
        let ids: Vec<WindowId> = self.windows.values().filter(|w| w.profile == slug).map(|w| w.id).collect();
        for wid in ids {
            if let Some(w) = self.windows.get_mut(&wid) {
                w.bookmarks_bar = bar;
                w.layout();
                let _ = w.toolbar.evaluate_script(&js);
                let _ = w.popup.evaluate_script(&js);
                for t in w.tabs.iter().filter(|t| t.page.is_some()) {
                    let _ = t.view.evaluate_script(&js);
                }
            }
            self.push_state(wid);
        }
    }

    /// Starts closing a window. Returns false when the whole app should exit.
    pub fn close_window(&mut self, wid: WindowId) -> bool {
        let Some(w) = self.windows.get_mut(&wid) else { return !self.windows.is_empty() };
        w.hide_popup();
        w.window.set_visible(false);
        let (slug, private) = (w.profile.clone(), w.private);
        let last_of_profile = !self.windows.values().any(|o| o.id != wid && o.profile == slug && !o.private);
        let clear = !private && last_of_profile && self.profiles.get(&slug).is_some_and(|p| p.store.settings.clear_on_exit);
        if clear {
            if let Some(rt) = self.profiles.get_mut(&slug) {
                rt.store.history.clear();
                rt.store.save_history();
            }
            if let Some(view) = self.windows.get(&wid).and_then(|w| w.tabs.first()).map(|t| &t.view) {
                let proxy = self.proxy.clone();
                let s = slug.clone();
                self.closing.entry(slug.clone()).or_default().push(wid);
                engine::clear_data(view, true, true, Box::new(move |_| {
                    let _ = proxy.send_event(UserEvent::ProfileCleared { profile: s });
                }));
                return true;
            }
        }
        self.finish_close(wid)
    }

    pub fn finish_close(&mut self, wid: WindowId) -> bool {
        if let Some(w) = self.windows.remove(&wid) {
            let dir: Option<PathBuf> = w.private.then(|| w.data_dir.clone());
            drop(w);
            if let Some(dir) = dir {
                wipe(&dir);
            }
        }
        !self.windows.is_empty() || !self.closing.is_empty()
    }
}

/// Deletes a folder, retrying while WebView2 processes release their file locks.
pub fn wipe(dir: &std::path::Path) {
    for _ in 0..20 {
        if std::fs::remove_dir_all(dir).is_ok() || !dir.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}
