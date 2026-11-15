//! One browser window: its toolbar, drop-down popup and tabs.

use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use tao::dpi::LogicalSize;
use tao::platform::windows::WindowExtWindows;
use tao::window::{Fullscreen, Window, WindowId};
use windows::Win32::Foundation::HWND;
use wry::{dpi, Rect, WebView};

use crate::engine;

pub const TABS_H: f64 = 38.0;
pub const NAV_H: f64 = 46.0;
pub const BOOKMARKS_H: f64 = 30.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    NewTab,
    History,
    Bookmarks,
    Downloads,
    Passwords,
    Settings,
    Extensions,
    Profiles,
    Help,
}

impl Page {
    pub const ALL: [Page; 9] = [
        Page::NewTab, Page::History, Page::Bookmarks, Page::Downloads, Page::Passwords,
        Page::Settings, Page::Extensions, Page::Profiles, Page::Help,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Page::NewTab => "newtab",
            Page::History => "history",
            Page::Bookmarks => "bookmarks",
            Page::Downloads => "downloads",
            Page::Passwords => "passwords",
            Page::Settings => "settings",
            Page::Extensions => "extensions",
            Page::Profiles => "profiles",
            Page::Help => "help",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::NewTab => "New Tab",
            Page::History => "History",
            Page::Bookmarks => "Bookmarks",
            Page::Downloads => "Downloads",
            Page::Passwords => "Passwords",
            Page::Settings => "Settings",
            Page::Extensions => "Extensions and Themes",
            Page::Profiles => "Profiles",
            Page::Help => "Help and Shortcuts",
        }
    }

    pub fn url(self) -> String {
        format!("veil://{}", self.slug())
    }

    pub fn from_url(url: &str) -> Option<Page> {
        let rest = url.trim().strip_prefix("veil://")?.trim_end_matches('/');
        Page::ALL.into_iter().find(|p| p.slug() == rest)
    }
}

pub struct Tab {
    pub id: u32,
    pub page: Option<Page>,
    pub view: WebView,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub blocked: Arc<AtomicU32>,
    pub favicon: String,
}

pub struct BrowserWindow {
    pub window: Window,
    pub id: WindowId,
    pub profile: String,
    pub private: bool,
    pub data_dir: PathBuf,
    pub toolbar: WebView,
    pub popup: WebView,
    pub popup_mode: Option<String>,
    pub popup_geom: (f64, f64, f64),
    pub tabs: Vec<Tab>,
    pub active: u32,
    pub closed: Vec<String>,
    pub bookmarks_bar: bool,
    pub fullscreen: bool,
}

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: dpi::LogicalPosition::new(x, y).into(),
        size: dpi::LogicalSize::new(w.max(0.0), h.max(0.0)).into(),
    }
}

impl BrowserWindow {
    pub fn hwnd(&self) -> HWND {
        HWND(self.window.hwnd() as _)
    }

    pub fn size(&self) -> LogicalSize<f64> {
        self.window.inner_size().to_logical(self.window.scale_factor())
    }

    pub fn toolbar_height(&self) -> f64 {
        if self.fullscreen {
            0.0
        } else {
            TABS_H + NAV_H + if self.bookmarks_bar { BOOKMARKS_H } else { 0.0 }
        }
    }

    pub fn content_rect(&self) -> Rect {
        let s = self.size();
        let top = self.toolbar_height();
        rect(0.0, top, s.width, s.height - top)
    }

    pub fn layout(&self) {
        let s = self.size();
        let _ = self.toolbar.set_bounds(rect(0.0, 0.0, s.width, self.toolbar_height()));
        let _ = self.toolbar.set_visible(!self.fullscreen);
        let content = self.content_rect();
        for tab in &self.tabs {
            let _ = tab.view.set_bounds(content);
        }
    }

    pub fn tab(&self, id: u32) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: u32) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tab(self.active)
    }

    pub fn active_web_view(&self) -> Option<&WebView> {
        self.active_tab().filter(|t| t.page.is_none()).map(|t| &t.view)
    }

    pub fn show_tab(&mut self, id: u32) {
        if self.tab(id).is_none() {
            return;
        }
        self.active = id;
        for tab in &self.tabs {
            let _ = tab.view.set_visible(tab.id == id);
        }
        if let Some(tab) = self.active_tab() {
            let _ = tab.view.focus();
        }
    }

    pub fn select_index(&mut self, index: usize) {
        if let Some(id) = self.tabs.get(index).map(|t| t.id) {
            self.show_tab(id);
        }
    }

    pub fn cycle(&mut self, step: isize) {
        let n = self.tabs.len() as isize;
        if let Some(i) = self.tabs.iter().position(|t| t.id == self.active) {
            let next = self.tabs[((i as isize + step).rem_euclid(n)) as usize].id;
            self.show_tab(next);
        }
    }

    /// Removes a tab. Returns false when no tabs remain.
    pub fn remove_tab(&mut self, id: u32) -> bool {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else { return true };
        let tab = self.tabs.remove(i);
        if tab.page.is_none() && !self.private && !tab.url.is_empty() {
            self.closed.push(tab.url.clone());
        }
        drop(tab);
        if self.tabs.is_empty() {
            return false;
        }
        if self.active == id {
            let next = self.tabs[i.min(self.tabs.len() - 1)].id;
            self.show_tab(next);
        }
        true
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.window.set_fullscreen(if self.fullscreen { Some(Fullscreen::Borderless(None)) } else { None });
        self.hide_popup();
        self.layout();
    }

    /// Shows the shared drop-down popup in `mode` at the given position.
    pub fn show_popup(&mut self, mode: &str, data: &serde_json::Value, x: f64, y: f64, w: f64, h: f64, focus: bool) {
        let s = self.size();
        let w = w.min(s.width - 8.0);
        let x = x.clamp(4.0, (s.width - w - 4.0).max(4.0));
        let h = h.min(s.height - y - 6.0);
        let script = format!("window.veil && window.veil.show({}, {})", serde_json::to_string(mode).unwrap(), data);
        let _ = self.popup.evaluate_script(&script);
        let _ = self.popup.set_bounds(rect(x, y, w, h));
        self.popup_geom = (x, y, w);
        let _ = self.popup.set_visible(true);
        engine::raise(&self.popup, self.hwnd());
        if focus {
            let _ = self.popup.focus();
        }
        self.popup_mode = Some(mode.to_string());
    }

    /// Fits the popup's height to its content (reported by the popup itself).
    pub fn fit_popup(&self, content_h: f64) {
        if self.popup_mode.is_none() {
            return;
        }
        let (x, y, w) = self.popup_geom;
        let h = content_h.min(self.size().height - y - 6.0);
        let _ = self.popup.set_bounds(rect(x, y, w, h));
    }

    pub fn hide_popup(&mut self) {
        if self.popup_mode.take().is_some() {
            let _ = self.popup.set_visible(false);
            let _ = self.popup.evaluate_script("window.veil && window.veil.hide()");
        }
    }

    pub fn eval_toolbar(&self, js: &str) {
        let _ = self.toolbar.evaluate_script(js);
    }
}
