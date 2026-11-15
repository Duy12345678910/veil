//! Executes browser actions from the menu, toolbar buttons and keyboard shortcuts.

use tao::event_loop::EventLoopWindowTarget;
use tao::window::WindowId;

use crate::app::App;
use crate::engine;
use crate::keys::Action;
use crate::views::UserEvent;
use crate::window::{Page, NAV_H, TABS_H};

const ZOOM_STEPS: &[f64] = &[0.3, 0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

impl App {
    /// Runs an action for a window. Returns false when the app should exit.
    pub fn action(&mut self, wid: WindowId, action: Action, target: &EventLoopWindowTarget<UserEvent>) -> bool {
        use Action::*;
        let Some(w) = self.windows.get_mut(&wid) else { return true };
        if !matches!(action, FindNext | FindPrev | Menu) {
            w.hide_popup();
        }
        let active = w.active;
        let profile = w.profile.clone();
        match action {
            NewTab => self.open_tab(wid, &Page::NewTab.url()),
            NewWindow => self.open_window(target, &profile, false, None),
            NewPrivateWindow => self.open_window(target, &profile, true, None),
            CloseTab => {
                if !w.remove_tab(active) {
                    return self.close_window(wid);
                }
                self.push_state(wid);
            }
            CloseWindow => return self.close_window(wid),
            ReopenTab => {
                if let Some(url) = w.closed.pop() {
                    self.open_tab(wid, &url);
                }
            }
            NextTab | PrevTab | SelectTab(_) | LastTab => {
                match action {
                    NextTab => w.cycle(1),
                    PrevTab => w.cycle(-1),
                    SelectTab(i) => w.select_index(i as usize),
                    _ => w.select_index(w.tabs.len().saturating_sub(1)),
                }
                self.push_state(wid);
            }
            FocusUrl => {
                let _ = w.toolbar.focus();
                w.eval_toolbar("window.veil && window.veil.focusUrl()");
            }
            Reload | HardReload => {
                if let Some(t) = w.active_tab() {
                    if action == HardReload && t.page.is_none() {
                        let _ = t.view.evaluate_script("location.reload()");
                    } else {
                        let _ = t.view.reload();
                    }
                }
            }
            Stop => w.active_web_view().into_iter().for_each(engine::stop),
            Back => w.active_web_view().into_iter().for_each(engine::go_back),
            Forward => w.active_web_view().into_iter().for_each(engine::go_forward),
            Home => self.navigate(wid, active, &Page::NewTab.url()),
            BookmarkPage => self.bookmark_current(wid),
            ToggleBookmarksBar => {
                if let Some(rt) = self.profiles.get_mut(&profile) {
                    rt.store.settings.show_bookmarks_bar = !rt.store.settings.show_bookmarks_bar;
                    rt.store.save_settings();
                }
                self.refresh_profile_windows(&profile);
            }
            History => self.open_page(wid, Page::History),
            Bookmarks => self.open_page(wid, Page::Bookmarks),
            Downloads => self.open_page(wid, Page::Downloads),
            Passwords => self.open_page(wid, Page::Passwords),
            Settings => self.open_page(wid, Page::Settings),
            Extensions => self.open_page(wid, Page::Extensions),
            Profiles => self.open_page(wid, Page::Profiles),
            Help => self.open_page(wid, Page::Help),
            ClearData => {
                self.open_page(wid, Page::Settings);
                if let Some(id) = self.windows.get(&wid).map(|w| w.active) {
                    self.reply(wid, crate::views::Source::Tab(id), "scroll", &serde_json::json!("privacy"));
                }
            }
            Print => w.active_web_view().into_iter().for_each(engine::print),
            SavePage => w.active_web_view().into_iter().for_each(engine::save_as),
            Find => {
                if w.active_web_view().is_some() {
                    let s = w.size();
                    let top = w.toolbar_height();
                    w.show_popup("find", &serde_json::json!({}), s.width - 440.0, top + 6.0, 430.0, 52.0, true);
                }
            }
            FindNext | FindPrev => {
                if w.popup_mode.as_deref() == Some("find") {
                    let _ = w.popup.evaluate_script(&format!("window.veil && window.veil.findStep({})", action == FindPrev));
                } else {
                    return self.action(wid, Find, target);
                }
            }
            ZoomIn | ZoomOut | ZoomReset => {
                if let Some(t) = w.active_tab() {
                    let z = engine::zoom(&t.view);
                    let next = match action {
                        ZoomIn => ZOOM_STEPS.iter().copied().find(|s| *s > z + 0.01).unwrap_or(3.0),
                        ZoomOut => ZOOM_STEPS.iter().rev().copied().find(|s| *s < z - 0.01).unwrap_or(0.3),
                        _ => 1.0,
                    };
                    engine::set_zoom(&t.view, next);
                    if w.popup_mode.as_deref() == Some("menu") {
                        let _ = w.popup.evaluate_script(&format!("window.veil && window.veil.zoom({next})"));
                    }
                }
            }
            Fullscreen => w.toggle_fullscreen(),
            DevTools => {
                if let Some(t) = w.active_tab() {
                    engine::open_devtools(&t.view);
                }
            }
            ViewSource => {
                if let Some(url) = w.active_tab().filter(|t| t.page.is_none()).map(|t| t.url.clone()) {
                    self.open_tab(wid, &format!("view-source:{url}"));
                }
            }
            Menu => self.show_menu(wid),
            Exit => {
                let ids: Vec<WindowId> = self.windows.keys().copied().collect();
                let mut alive = true;
                for id in ids {
                    alive = self.close_window(id);
                }
                return alive;
            }
        }
        true
    }

    pub fn show_menu(&mut self, wid: WindowId) {
        let Some(w) = self.windows.get_mut(&wid) else { return };
        if w.popup_mode.as_deref() == Some("menu") {
            w.hide_popup();
            return;
        }
        let zoom = w.active_tab().map(|t| engine::zoom(&t.view)).unwrap_or(1.0);
        let Some(rt) = self.profiles.get(&w.profile) else { return };
        let data = serde_json::json!({
            "zoom": zoom, "private": w.private, "web": w.active_web_view().is_some(),
            "profile": { "name": rt.info.name, "color": rt.info.color },
            "bookmarksBar": w.bookmarks_bar,
        });
        let s = w.size();
        w.show_popup("menu", &data, s.width - 338.0, TABS_H + NAV_H - 4.0, 330.0, 700.0, true);
    }

    pub fn bookmark_current(&mut self, wid: WindowId) {
        let Some(w) = self.windows.get(&wid) else { return };
        let Some(t) = w.active_tab().filter(|t| t.page.is_none()) else { return };
        let (title, url) = (t.title.clone(), t.url.clone());
        let Some(rt) = self.prof_mut(wid) else { return };
        let id = rt.store.add_bookmark(&title, &url);
        let data = serde_json::json!({ "id": id, "title": title, "url": url });
        let Some(w) = self.windows.get_mut(&wid) else { return };
        let s = w.size();
        w.show_popup("bookmark", &data, s.width - 420.0, TABS_H + NAV_H - 4.0, 340.0, 196.0, true);
        self.push_state(wid);
    }
}
