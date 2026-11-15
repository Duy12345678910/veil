//! Browser actions and the Firefox/Chrome-style keyboard shortcuts that trigger them.

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    NewTab,
    NewWindow,
    NewPrivateWindow,
    CloseTab,
    CloseWindow,
    ReopenTab,
    NextTab,
    PrevTab,
    SelectTab(u8),
    LastTab,
    FocusUrl,
    Reload,
    HardReload,
    Stop,
    Back,
    Forward,
    Home,
    BookmarkPage,
    ToggleBookmarksBar,
    History,
    Bookmarks,
    Downloads,
    Passwords,
    Settings,
    Extensions,
    Profiles,
    Help,
    ClearData,
    Print,
    SavePage,
    Find,
    FindNext,
    FindPrev,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
    DevTools,
    ViewSource,
    Menu,
    Exit,
}

/// Shortcut table shown on the Help page: (keys, description).
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+T", "New tab"),
    ("Ctrl+N", "New window"),
    ("Ctrl+Shift+P / Ctrl+Shift+N", "New private window"),
    ("Ctrl+W / Ctrl+F4", "Close tab"),
    ("Ctrl+Shift+W", "Close window"),
    ("Ctrl+Shift+T", "Reopen closed tab"),
    ("Ctrl+Tab / Ctrl+PgDn", "Next tab"),
    ("Ctrl+Shift+Tab / Ctrl+PgUp", "Previous tab"),
    ("Ctrl+1 … Ctrl+8", "Go to tab 1–8"),
    ("Ctrl+9", "Go to last tab"),
    ("Ctrl+L / Alt+D / F6", "Focus address bar"),
    ("F5 / Ctrl+R", "Reload"),
    ("Ctrl+F5 / Ctrl+Shift+R", "Reload, bypassing cache"),
    ("Alt+← / Alt+→", "Back / Forward"),
    ("Alt+Home", "Home page"),
    ("Ctrl+D", "Bookmark this page"),
    ("Ctrl+Shift+B", "Show/hide bookmarks bar"),
    ("Ctrl+Shift+O", "Bookmarks"),
    ("Ctrl+H", "History"),
    ("Ctrl+J", "Downloads"),
    ("Ctrl+Shift+A", "Extensions and themes"),
    ("Ctrl+Shift+Delete", "Clear browsing data"),
    ("Ctrl+P", "Print"),
    ("Ctrl+S", "Save page as"),
    ("Ctrl+F", "Find in page"),
    ("F3 / Shift+F3", "Find next / previous"),
    ("Ctrl+= / Ctrl+- / Ctrl+0", "Zoom in / out / reset"),
    ("F11", "Full screen"),
    ("F12 / Ctrl+Shift+I", "Developer tools"),
    ("Ctrl+U", "View page source"),
    ("Alt+F / Alt+E", "Open menu"),
    ("Ctrl+Shift+Q", "Exit Veil"),
];

/// Returns (action, allow_repeat) for a key press with the given modifiers.
pub fn map(vk: u32, ctrl: bool, shift: bool, alt: bool) -> Option<(Action, bool)> {
    use Action::*;
    let key = |c: char| c as u32;
    let a = match (ctrl, shift, alt, vk) {
        (true, false, false, k) if k == key('T') => NewTab,
        (true, false, false, k) if k == key('N') => NewWindow,
        (true, true, false, k) if k == key('P') || k == key('N') => NewPrivateWindow,
        (true, false, false, k) if k == key('W') => CloseTab,
        (true, false, false, 0x73) => CloseTab, // F4
        (true, true, false, k) if k == key('W') => CloseWindow,
        (true, true, false, k) if k == key('T') => ReopenTab,
        (true, false, false, 0x09) | (true, false, false, 0x22) => return Some((NextTab, true)),
        (true, true, false, 0x09) | (true, false, false, 0x21) => return Some((PrevTab, true)),
        (true, false, false, k @ 0x31..=0x38) => SelectTab((k - 0x31) as u8),
        (true, false, false, 0x39) => LastTab,
        (true, false, false, k) if k == key('L') => FocusUrl,
        (false, false, true, k) if k == key('D') => FocusUrl,
        (false, false, false, 0x75) => FocusUrl, // F6
        (false, false, false, 0x74) => Reload,   // F5
        (true, false, false, k) if k == key('R') => Reload,
        (true, false, false, 0x74) => HardReload,
        (true, true, false, k) if k == key('R') => HardReload,
        (false, false, true, 0x25) => Back,
        (false, false, true, 0x27) => Forward,
        (false, false, true, 0x24) => Home,
        (true, false, false, k) if k == key('D') => BookmarkPage,
        (true, true, false, k) if k == key('B') => ToggleBookmarksBar,
        (true, true, false, k) if k == key('O') => Bookmarks,
        (true, false, false, k) if k == key('H') => History,
        (true, false, false, k) if k == key('J') => Downloads,
        (true, true, false, k) if k == key('A') => Extensions,
        (true, true, false, 0x2E) => ClearData, // Delete
        (true, false, false, k) if k == key('P') => Print,
        (true, false, false, k) if k == key('S') => SavePage,
        (true, false, false, k) if k == key('F') => Find,
        (false, false, false, 0x72) => return Some((FindNext, true)), // F3
        (false, true, false, 0x72) => return Some((FindPrev, true)),
        (true, _, false, 0xBB) | (true, _, false, 0x6B) => return Some((ZoomIn, true)),
        (true, _, false, 0xBD) | (true, _, false, 0x6D) => return Some((ZoomOut, true)),
        (true, false, false, 0x30) | (true, false, false, 0x60) => ZoomReset,
        (false, false, false, 0x7A) => Fullscreen, // F11
        (false, false, false, 0x7B) => DevTools,   // F12
        (true, true, false, k) if k == key('I') => DevTools,
        (true, false, false, k) if k == key('U') => ViewSource,
        (false, false, true, k) if k == key('F') || k == key('E') => Menu,
        (true, true, false, k) if k == key('Q') => Exit,
        _ => return None,
    };
    Some((a, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_shortcuts() {
        assert_eq!(map('T' as u32, true, false, false), Some((Action::NewTab, false)));
        assert_eq!(map('P' as u32, true, true, false), Some((Action::NewPrivateWindow, false)));
        assert_eq!(map(0x33, true, false, false), Some((Action::SelectTab(2), false)));
        assert_eq!(map('C' as u32, true, false, false), None); // copy must pass through
        assert_eq!(map('A' as u32, true, false, false), None); // select-all must pass through
    }
}
