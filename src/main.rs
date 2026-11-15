#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Veil — a privacy-focused web browser by Claude, built in Rust on WebView2.

mod actions;
mod app;
mod blocker;
mod commands;
mod engine;
mod keys;
mod passwords;
mod shield;
mod store;
mod ui;
mod views;
mod window;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};

use app::App;
use store::{now_ms, Download};
use views::UserEvent;

/// Removes private-window folders left behind by a crash.
fn clean_temp() {
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("veil-private-") || name.starts_with("veil-session-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

impl App {
    /// Handles one app event. Returns false when the app should exit.
    fn handle(&mut self, event: UserEvent, target: &EventLoopWindowTarget<UserEvent>) -> bool {
        match event {
            UserEvent::Ipc { wid, src, body } => return self.handle_ipc(wid, src, &body, target),
            UserEvent::Shortcut { wid, action } => return self.action(wid, action, target),
            UserEvent::Title { wid, tab, title } => {
                let Some(w) = self.windows.get_mut(&wid) else { return true };
                let (private, slug) = (w.private, w.profile.clone());
                let Some(t) = w.tab_mut(tab) else { return true };
                t.title = title.clone();
                if t.page.is_none() {
                    let fav = engine::favicon(&t.view);
                    if !fav.is_empty() {
                        t.favicon = fav;
                    }
                }
                let url = t.url.clone();
                if !private {
                    if let Some(rt) = self.profiles.get_mut(&slug) {
                        rt.store.update_title(&url, &title);
                    }
                }
                self.push_state(wid);
            }
            UserEvent::Load { wid, tab, started, url } => {
                let Some(w) = self.windows.get_mut(&wid) else { return true };
                let (private, slug) = (w.private, w.profile.clone());
                let Some(t) = w.tab_mut(tab) else { return true };
                if t.page.is_some() {
                    return true;
                }
                t.loading = started;
                t.url = url.clone();
                if started {
                    t.blocked.store(0, std::sync::atomic::Ordering::Relaxed);
                } else {
                    t.favicon = engine::favicon(&t.view);
                    let title = t.title.clone();
                    if !private {
                        if let Some(rt) = self.profiles.get_mut(&slug) {
                            rt.store.record_visit(&url, &title);
                        }
                    }
                }
                self.push_state(wid);
            }
            UserEvent::OpenUrl { wid, url } => self.open_tab(wid, &url),
            UserEvent::Redirect { wid, tab, url } => {
                if let Some(t) = self.windows.get(&wid).and_then(|w| w.tab(tab)) {
                    let _ = t.view.load_url(&url);
                }
            }
            UserEvent::Blocked { wid } => self.push_state(wid),
            UserEvent::DownloadStarted { profile, url, path } => {
                let rt = self.profile(&profile);
                let id = now_ms() * 10 + (rt.store.downloads.len() as u64 % 10);
                rt.store.downloads.push(Download { id, url, path: path.to_string_lossy().into(), state: "active".into(), time: now_ms() });
                rt.store.save_downloads();
                self.push_profile(&profile);
            }
            UserEvent::DownloadDone { profile, url, path, ok } => {
                let rt = self.profile(&profile);
                if let Some(d) = rt.store.downloads.iter_mut().rev().find(|d| d.url == url && d.state == "active") {
                    d.state = if ok { "done" } else { "failed" }.into();
                    if let Some(p) = path {
                        d.path = p.to_string_lossy().into();
                    }
                }
                rt.store.save_downloads();
                self.push_profile(&profile);
            }
            UserEvent::Reply { wid, src, what, data } => self.reply(wid, src, &what, &data),
            UserEvent::ProfileCleared { profile } => {
                for wid in self.closing.remove(&profile).unwrap_or_default() {
                    self.finish_close(wid);
                }
                return !self.windows.is_empty();
            }
        }
        true
    }

    fn push_profile(&self, slug: &str) {
        for w in self.windows.values().filter(|w| w.profile == slug) {
            self.push_state(w.id);
        }
    }
}

fn main() {
    clean_temp();
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let mut app = App::new(event_loop.create_proxy());
    let start = std::env::args().nth(1);
    let default = app.list.profiles[0].slug.clone();
    app.open_window(&event_loop, &default, false, start);
    if app.windows.is_empty() {
        eprintln!("veil: failed to open a window (is the WebView2 Runtime installed?)");
        return;
    }

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let alive = match event {
            Event::UserEvent(e) => app.handle(e, target),
            Event::WindowEvent { window_id, event, .. } => match event {
                WindowEvent::CloseRequested => app.close_window(window_id),
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(w) = app.windows.get_mut(&window_id) {
                        w.hide_popup();
                        w.layout();
                    }
                    true
                }
                WindowEvent::Moved(_) => {
                    if let Some(w) = app.windows.get_mut(&window_id) {
                        w.hide_popup();
                    }
                    true
                }
                _ => true,
            },
            _ => true,
        };
        if !alive && app.closing.is_empty() {
            app.windows.clear();
            *control_flow = ControlFlow::Exit;
        }
    });
}
