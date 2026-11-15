//! Builders for every webview Veil creates: toolbar, popup, internal pages and web tabs.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use tao::event_loop::EventLoopProxy;
use tao::window::{Window, WindowId};
use wry::{NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

use crate::keys::Action;
use crate::store::Flags;
use crate::{blocker, shield};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
    Toolbar,
    Popup,
    Tab(u32),
}

#[derive(Debug)]
pub enum UserEvent {
    Ipc { wid: WindowId, src: Source, body: String },
    Shortcut { wid: WindowId, action: Action },
    Title { wid: WindowId, tab: u32, title: String },
    Load { wid: WindowId, tab: u32, started: bool, url: String },
    OpenUrl { wid: WindowId, url: String },
    Redirect { wid: WindowId, tab: u32, url: String },
    Blocked { wid: WindowId },
    DownloadStarted { profile: String, url: String, path: PathBuf },
    DownloadDone { profile: String, url: String, path: Option<PathBuf>, ok: bool },
    Reply { wid: WindowId, src: Source, what: String, data: serde_json::Value },
    ProfileCleared { profile: String },
}

/// Chromium switches shared by every webview of a profile.
pub fn browser_args(block_third_party_cookies: bool) -> String {
    let mut args = String::from(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,AutofillServerCommunication,\
msEdgeTranslate,msImplicitSignin,InterestFeedContentSuggestions \
--disable-background-networking --disable-sync --disable-domain-reliability --disable-breakpad \
--no-pings --force-webrtc-ip-handling-policy=disable_non_proxied_udp \
--webrtc-ip-handling-policy=disable_non_proxied_udp",
    );
    if block_third_party_cookies {
        args.push_str(" --test-third-party-cookie-phaseout");
    }
    args
}

const PRIVACY_SCRIPT: &str = r#"
(() => {
  try { Object.defineProperty(Navigator.prototype, 'globalPrivacyControl', { get: () => true, configurable: true }); } catch (_) {}
  try { delete Navigator.prototype.getBattery; } catch (_) {}
})();
"#;

pub struct Ctx<'a> {
    pub window: &'a Window,
    pub wid: WindowId,
    pub proxy: &'a EventLoopProxy<UserEvent>,
    pub data_dir: &'a Path,
    pub private: bool,
    pub args: &'a str,
    pub profile: &'a str,
}

fn base<'a>(ctx: &Ctx, wc: &'a mut WebContext) -> WebViewBuilder<'a> {
    WebViewBuilder::new_with_web_context(wc)
        .with_incognito(ctx.private)
        .with_additional_browser_args(ctx.args)
        .with_browser_extensions_enabled(!ctx.private)
        .with_devtools(true)
        .with_general_autofill_enabled(false)
}

fn finish(ctx: &Ctx, view: WebView) -> wry::Result<WebView> {
    let (proxy, wid) = (ctx.proxy.clone(), ctx.wid);
    if let Err(e) = shield::install_shortcuts(&view, move |action| {
        let _ = proxy.send_event(UserEvent::Shortcut { wid, action });
    }) {
        eprintln!("veil: shortcuts unavailable: {e}");
    }
    Ok(view)
}

fn ipc(ctx: &Ctx, src: Source) -> impl Fn(wry::http::Request<String>) + 'static {
    let (proxy, wid) = (ctx.proxy.clone(), ctx.wid);
    move |req| {
        let _ = proxy.send_event(UserEvent::Ipc { wid, src, body: req.body().clone() });
    }
}

/// Trusted UI webviews (toolbar, popup, internal pages) never load web content.
fn ui_nav(ctx: &Ctx) -> impl Fn(String) -> bool + 'static {
    let (proxy, wid) = (ctx.proxy.clone(), ctx.wid);
    move |url| {
        if url.starts_with("http://") || url.starts_with("https://") {
            let _ = proxy.send_event(UserEvent::OpenUrl { wid, url });
            return false;
        }
        true
    }
}

pub fn ui_view(ctx: &Ctx, html: String, bounds: Rect, src: Source, visible: bool) -> wry::Result<WebView> {
    let mut wc = WebContext::new(Some(ctx.data_dir.to_path_buf()));
    let proxy = ctx.proxy.clone();
    let wid = ctx.wid;
    let view = base(ctx, &mut wc)
        .with_html(html)
        .with_bounds(bounds)
        .with_visible(visible)
        .with_ipc_handler(ipc(ctx, src))
        .with_navigation_handler(ui_nav(ctx))
        .with_new_window_req_handler(move |url, _| {
            let _ = proxy.send_event(UserEvent::OpenUrl { wid, url });
            NewWindowResponse::Deny
        })
        .build_as_child(ctx.window)?;
    finish(ctx, view)
}

pub fn web_view(
    ctx: &Ctx,
    tab: u32,
    url: &str,
    bounds: Rect,
    flags: Arc<Flags>,
    blocked: Arc<AtomicU32>,
    strict: bool,
) -> wry::Result<WebView> {
    let mut wc = WebContext::new(Some(ctx.data_dir.to_path_buf()));
    let wid = ctx.wid;
    let p = || ctx.proxy.clone();
    let (p_nav, p_title, p_load, p_popup, p_dl, p_dl2) = (p(), p(), p(), p(), p(), p());
    let nav_flags = flags.clone();
    let record = !ctx.private;
    let (prof1, prof2) = (ctx.profile.to_string(), ctx.profile.to_string());

    let view = base(ctx, &mut wc)
        .with_url(url)
        .with_bounds(bounds)
        .with_autoplay(false)
        .with_initialization_script(PRIVACY_SCRIPT)
        .with_navigation_handler(move |url| {
            if Flags::get(&nav_flags.block_trackers) && blocker::is_blocked_url(&url) {
                return false;
            }
            let mut fixed = url.clone();
            if Flags::get(&nav_flags.https_only) {
                fixed = blocker::https_upgrade(&fixed).unwrap_or(fixed);
            }
            if Flags::get(&nav_flags.strip_tracking) {
                fixed = blocker::strip_tracking_params(&fixed).unwrap_or(fixed);
            }
            if fixed != url {
                let _ = p_nav.send_event(UserEvent::Redirect { wid, tab, url: fixed });
                return false;
            }
            true
        })
        .with_document_title_changed_handler(move |title| {
            let _ = p_title.send_event(UserEvent::Title { wid, tab, title });
        })
        .with_on_page_load_handler(move |event, url| {
            let started = matches!(event, PageLoadEvent::Started);
            let _ = p_load.send_event(UserEvent::Load { wid, tab, started, url });
        })
        .with_new_window_req_handler(move |url, _| {
            if !blocker::is_blocked_url(&url) {
                let _ = p_popup.send_event(UserEvent::OpenUrl { wid, url });
            }
            NewWindowResponse::Deny
        })
        .with_download_started_handler(move |url, path| {
            if record {
                let _ = p_dl.send_event(UserEvent::DownloadStarted { profile: prof1.clone(), url, path: path.clone() });
            }
            true
        })
        .with_download_completed_handler(move |url, path, ok| {
            if record {
                let _ = p_dl2.send_event(UserEvent::DownloadDone { profile: prof2.clone(), url, path, ok });
            }
        })
        .build_as_child(ctx.window)?;

    let p_block = ctx.proxy.clone();
    if let Err(e) = shield::install_blocker(&view, flags, blocked, strict, move || {
        let _ = p_block.send_event(UserEvent::Blocked { wid });
    }) {
        eprintln!("veil: failed to install content blocker: {e}");
    }
    finish(ctx, view)
}
