//! Thin wrappers over WebView2 COM APIs: navigation, zoom, print, cookies, extensions, data clearing.

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
    ClearBrowsingDataCompletedHandler, GetCookiesCompletedHandler, ProfileAddBrowserExtensionCompletedHandler,
    ProfileGetBrowserExtensionsCompletedHandler, ShowSaveAsUICompletedHandler,
};
use windows::core::{Interface, HSTRING, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetParent, SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
use wry::{WebView, WebViewExtWindows};

pub type Done = Box<dyn FnOnce(serde_json::Value)>;

fn core(v: &WebView) -> ICoreWebView2 {
    v.webview()
}

fn profile(v: &WebView) -> windows::core::Result<ICoreWebView2Profile> {
    unsafe { core(v).cast::<ICoreWebView2_13>()?.Profile() }
}

pub fn go_back(v: &WebView) {
    unsafe { let _ = core(v).GoBack(); }
}
pub fn go_forward(v: &WebView) {
    unsafe { let _ = core(v).GoForward(); }
}
pub fn stop(v: &WebView) {
    unsafe { let _ = core(v).Stop(); }
}
pub fn open_devtools(v: &WebView) {
    unsafe { let _ = core(v).OpenDevToolsWindow(); }
}
pub fn print(v: &WebView) {
    unsafe {
        if let Ok(c) = core(v).cast::<ICoreWebView2_16>() {
            let _ = c.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER);
        }
    }
}
pub fn save_as(v: &WebView) {
    unsafe {
        if let Ok(c) = core(v).cast::<ICoreWebView2_25>() {
            let _ = c.ShowSaveAsUI(&ShowSaveAsUICompletedHandler::create(Box::new(|_, _| Ok(()))));
        }
    }
}

pub fn zoom(v: &WebView) -> f64 {
    let mut z = 1.0;
    unsafe { let _ = v.controller().ZoomFactor(&mut z); }
    z
}
pub fn set_zoom(v: &WebView, z: f64) {
    unsafe { let _ = v.controller().SetZoomFactor(z); }
}

pub fn favicon(v: &WebView) -> String {
    unsafe {
        let Ok(c) = core(v).cast::<ICoreWebView2_15>() else { return String::new() };
        let mut p = PWSTR::null();
        if c.FaviconUri(&mut p).is_ok() { take_pwstr(p) } else { String::new() }
    }
}

/// Raises a webview's host window above its siblings (used for drop-down popups).
pub fn raise(v: &WebView, top_level: HWND) {
    unsafe {
        let mut hwnd = HWND::default();
        if v.controller().ParentWindow(&mut hwnd).is_err() {
            return;
        }
        while let Ok(parent) = GetParent(hwnd) {
            if parent == top_level || parent.is_invalid() {
                break;
            }
            hwnd = parent;
        }
        let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Clears browsing data for the whole profile behind `v`.
pub fn clear_data(v: &WebView, cookies: bool, cache: bool, done: Done) {
    let mut kinds = 0;
    if cookies {
        kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE.0;
    }
    if cache {
        kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0 | COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY.0;
    }
    let result = (|| unsafe {
        let p2 = profile(v)?.cast::<ICoreWebView2Profile2>()?;
        let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |r| {
            done(serde_json::json!({ "ok": r.is_ok() }));
            Ok(())
        }));
        p2.ClearBrowsingData(COREWEBVIEW2_BROWSING_DATA_KINDS(kinds), &handler)
    })();
    if let Err(e) = result {
        eprintln!("veil: clear data failed: {e}");
    }
}

fn cookie_manager(v: &WebView) -> windows::core::Result<ICoreWebView2CookieManager> {
    unsafe { core(v).cast::<ICoreWebView2_2>()?.CookieManager() }
}

/// Lists all cookies in the profile, grouped by domain. If `delete_domain` is set,
/// cookies for that domain (and its subdomains) are deleted first.
pub fn cookies(v: &WebView, delete_domain: Option<String>, done: Done) {
    let Ok(mgr) = cookie_manager(v) else { return };
    let mgr2 = mgr.clone();
    let handler = GetCookiesCompletedHandler::create(Box::new(move |r, list| {
        r?;
        let mut out: Vec<serde_json::Value> = Vec::new();
        if let Some(list) = list {
            unsafe {
                let mut n = 0u32;
                list.Count(&mut n)?;
                for i in 0..n {
                    let c = list.GetValueAtIndex(i)?;
                    let mut p = PWSTR::null();
                    c.Domain(&mut p)?;
                    let domain = take_pwstr(p);
                    let bare = domain.trim_start_matches('.').to_string();
                    if let Some(d) = &delete_domain {
                        if bare == *d || bare.ends_with(&format!(".{d}")) {
                            let _ = mgr2.DeleteCookie(&c);
                            continue;
                        }
                    }
                    c.Name(&mut p)?;
                    let name = take_pwstr(p);
                    let mut session = Default::default();
                    c.IsSession(&mut session)?;
                    out.push(serde_json::json!({ "domain": bare, "name": name, "session": session.as_bool() }));
                }
            }
        }
        done(serde_json::Value::Array(out));
        Ok(())
    }));
    unsafe {
        let _ = mgr.GetCookies(&HSTRING::new(), &handler);
    }
}

fn profile7(v: &WebView) -> windows::core::Result<ICoreWebView2Profile7> {
    profile(v)?.cast::<ICoreWebView2Profile7>()
}

/// Lists installed extensions; optionally removes or toggles one by id first.
pub fn extensions(v: &WebView, op: Option<(String, Option<bool>)>, done: Done) {
    let Ok(p7) = profile7(v) else {
        done(serde_json::json!({ "error": "Extensions are not available in this window." }));
        return;
    };
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |r, list| {
        r?;
        let mut out = Vec::new();
        if let Some(list) = list {
            unsafe {
                let mut n = 0u32;
                list.Count(&mut n)?;
                for i in 0..n {
                    let ext = list.GetValueAtIndex(i)?;
                    let mut p = PWSTR::null();
                    ext.Id(&mut p)?;
                    let id = take_pwstr(p);
                    ext.Name(&mut p)?;
                    let name = take_pwstr(p);
                    let mut enabled = Default::default();
                    ext.IsEnabled(&mut enabled)?;
                    let mut enabled = enabled.as_bool();
                    if let Some((target, action)) = &op {
                        if *target == id {
                            match action {
                                None => {
                                    let _ = ext.Remove(&BrowserExtensionRemoveCompletedHandler::create(Box::new(|_| Ok(()))));
                                    continue;
                                }
                                Some(on) => {
                                    let _ = ext.Enable(*on, &BrowserExtensionEnableCompletedHandler::create(Box::new(|_| Ok(()))));
                                    enabled = *on;
                                }
                            }
                        }
                    }
                    out.push(serde_json::json!({ "id": id, "name": name, "enabled": enabled }));
                }
            }
        }
        done(serde_json::json!({ "list": out }));
        Ok(())
    }));
    unsafe {
        let _ = p7.GetBrowserExtensions(&handler);
    }
}

pub fn add_extension(v: &WebView, folder: &str, done: Done) {
    let Ok(p7) = profile7(v) else {
        done(serde_json::json!({ "error": "Extensions are not available in this window." }));
        return;
    };
    let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |r, _| {
        done(match r {
            Ok(()) => serde_json::json!({ "ok": true }),
            Err(e) => serde_json::json!({ "error": format!("Could not load extension: {}", e.message()) }),
        });
        Ok(())
    }));
    unsafe {
        let _ = p7.AddBrowserExtension(&HSTRING::from(folder), &handler);
    }
}
