//! Assembles Veil's built-in HTML surfaces from the files in /ui.

use crate::window::Page;

const THEME: &str = include_str!("../ui/theme.css");
const ICONS: &str = include_str!("../ui/icons.js");
const LOGO: &str = include_str!("../assets/veil.svg");
const TOOLBAR: &str = include_str!("../ui/toolbar.html");
const POPUP: &str = include_str!("../ui/popup.html");
const NEWTAB: &str = include_str!("../ui/newtab.html");
const PAGE: &str = include_str!("../ui/page.html");
const PAGES_JS: &str = include_str!("../ui/pages.js");
const PAGES2_JS: &str = include_str!("../ui/pages2.js");

fn fill(template: &str, theme: &str, private: bool) -> String {
    let theme: String = theme.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    template
        .replace("/*THEME*/", THEME)
        .replace("/*ICONS*/", ICONS)
        .replace("{{LOGO}}", LOGO)
        .replace("{{THEME}}", &theme)
        .replace("{{PRIVATE}}", if private { "1" } else { "0" })
}

pub fn toolbar_html(theme: &str, private: bool) -> String {
    fill(TOOLBAR, theme, private)
}

pub fn popup_html(theme: &str, private: bool) -> String {
    fill(POPUP, theme, private)
}

pub fn page_html(page: Page, theme: &str, private: bool) -> String {
    if page == Page::NewTab {
        return fill(NEWTAB, theme, private);
    }
    let page_template = PAGE.replace("/*PAGES_JS*/", &format!("{PAGES_JS}\n{PAGES2_JS}"));
    fill(&page_template, theme, private)
        .replace("{{PAGE}}", page.slug())
        .replace("{{TITLE}}", page.title())
}
