//! WebView2-level hooks installed on every tab: request blocking, privacy headers,
//! hardened settings and keyboard shortcuts.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Profile3, ICoreWebView2Settings4, ICoreWebView2_13, ICoreWebView2_22,
    COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    COREWEBVIEW2_PHYSICAL_KEY_STATUS, COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
    COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
};
use webview2_com::{take_pwstr, AcceleratorKeyPressedEventHandler, WebResourceRequestedEventHandler};
use windows::core::{Interface, PWSTR};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows_core::w;
use wry::WebViewExtWindows;

use crate::blocker;
use crate::keys::{self, Action};
use crate::store::Flags;

/// Installs the content blocker, privacy headers and hardened settings on a web tab.
pub fn install_blocker(
    webview: &wry::WebView,
    flags: Arc<Flags>,
    counter: Arc<AtomicU32>,
    strict: bool,
    on_block: impl Fn() + 'static,
) -> windows::core::Result<()> {
    unsafe {
        let core = webview.webview();
        let env = webview.environment();

        let settings = core.Settings()?;
        if let Ok(s4) = settings.cast::<ICoreWebView2Settings4>() {
            s4.SetIsPasswordAutosaveEnabled(false)?;
            s4.SetIsGeneralAutofillEnabled(false)?;
        }
        if let Ok(p3) = core.cast::<ICoreWebView2_13>().and_then(|c| c.Profile()).and_then(|p| p.cast::<ICoreWebView2Profile3>()) {
            let level = if strict { COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT } else { COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED };
            let _ = p3.SetPreferredTrackingPreventionLevel(level);
        }

        match core.cast::<ICoreWebView2_22>() {
            Ok(c22) => c22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?,
            Err(_) => core.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?,
        }

        let mut token = 0i64;
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take_pwstr(uri);

                if Flags::get(&flags.block_trackers) && blocker::is_blocked_url(&uri) {
                    let response = env.CreateWebResourceResponse(None, 403, w!("Blocked by Veil"), w!(""))?;
                    args.SetResponse(&response)?;
                    counter.fetch_add(1, Ordering::Relaxed);
                    #[cfg(debug_assertions)]
                    eprintln!("veil: blocked {uri}");
                    on_block();
                } else if Flags::get(&flags.send_gpc) {
                    let headers = request.Headers()?;
                    headers.SetHeader(w!("Sec-GPC"), w!("1"))?;
                    headers.SetHeader(w!("DNT"), w!("1"))?;
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}

fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) < 0 }
}

/// Routes browser shortcuts pressed inside any webview (web page, toolbar or internal page).
pub fn install_shortcuts(webview: &wry::WebView, on_action: impl Fn(Action) + 'static) -> windows::core::Result<()> {
    unsafe {
        let controller = webview.controller();
        let mut token = 0i64;
        controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                    return Ok(());
                }
                let mut vk = 0u32;
                args.VirtualKey(&mut vk)?;
                let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                args.PhysicalKeyStatus(&mut status)?;
                let ctrl = key_down(VK_CONTROL.0 as i32);
                let shift = key_down(VK_SHIFT.0 as i32);
                let alt = key_down(VK_MENU.0 as i32);
                if let Some((action, repeatable)) = keys::map(vk, ctrl, shift, alt) {
                    args.SetHandled(true)?;
                    if repeatable || !status.WasKeyDown.as_bool() {
                        on_action(action);
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}
