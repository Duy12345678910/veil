//! Tracker/ad domain blocking, tracking-parameter stripping and address-bar parsing.

/// Built-in list of advertising, analytics and tracking domains.
/// A request is blocked when its host equals one of these or is a subdomain of one.
const BLOCKED_DOMAINS: &[&str] = &[
    // Google ads / analytics
    "doubleclick.net", "googleadservices.com", "googlesyndication.com", "google-analytics.com",
    "googletagmanager.com", "googletagservices.com", "adservice.google.com", "pagead2.googlesyndication.com",
    "analytics.google.com", "app-measurement.com", "2mdn.net", "adsense.com", "admob.com",
    // Meta / social trackers
    "connect.facebook.net", "pixel.facebook.com", "an.facebook.com", "graph.instagram.com",
    "analytics.twitter.com", "ads-twitter.com", "static.ads-twitter.com", "ads.linkedin.com",
    "px.ads.linkedin.com", "snap.licdn.com", "analytics.tiktok.com", "ads.tiktok.com",
    "sc-static.net", "tr.snapchat.com", "ct.pinterest.com", "ads.pinterest.com", "ads.reddit.com",
    "events.redditmedia.com", "alb.reddit.com",
    // Microsoft / Amazon / Yahoo ad networks
    "bat.bing.com", "clarity.ms", "ads.microsoft.com", "adnxs.com", "amazon-adsystem.com",
    "assoc-amazon.com", "advertising.com", "adtech.de", "ads.yahoo.com", "analytics.yahoo.com",
    "gemini.yahoo.com",
    // Analytics / session replay
    "hotjar.com", "hotjar.io", "mouseflow.com", "fullstory.com", "crazyegg.com", "luckyorange.com",
    "inspectlet.com", "logrocket.io", "smartlook.com", "mixpanel.com", "segment.io", "segment.com",
    "amplitude.com", "heapanalytics.com", "kissmetrics.com", "quantserve.com", "quantcount.com",
    "scorecardresearch.com", "chartbeat.com", "chartbeat.net", "newrelic.com", "nr-data.net",
    "statcounter.com", "histats.com", "clicky.com", "getclicky.com", "woopra.com", "optimizely.com",
    "mc.yandex.ru", "an.yandex.ru", "top-fwz1.mail.ru", "counter.yadro.ru", "hm.baidu.com",
    "cnzz.com", "branch.io", "app.link", "adjust.com", "appsflyer.com", "kochava.com",
    "omtrdc.net", "demdex.net", "everesttech.net", "2o7.net", "sc.omtrdc.net", "bluekai.com",
    "krxd.net", "exelator.com", "eyeota.net", "tapad.com", "agkn.com",
    // Ad exchanges / SSPs / DSPs
    "criteo.com", "criteo.net", "taboola.com", "outbrain.com", "outbrainimg.com", "revcontent.com",
    "mgid.com", "zemanta.com", "pubmatic.com", "rubiconproject.com", "openx.net", "casalemedia.com",
    "indexww.com", "contextweb.com", "smartadserver.com", "adform.net", "adsrvr.org", "mathtag.com",
    "turn.com", "rlcdn.com", "bidswitch.net", "sharethrough.com", "33across.com", "triplelift.com",
    "teads.tv", "yieldmo.com", "media.net", "moatads.com", "moatpixel.com", "adsafeprotected.com",
    "doubleverify.com", "serving-sys.com", "flashtalking.com", "spotxchange.com", "spotx.tv",
    "lijit.com", "sovrn.com", "gumgum.com", "undertone.com", "adroll.com", "perfectaudience.com",
    "quantcast.com", "zedo.com", "propellerads.com", "popads.net", "popcash.net", "adcash.com",
    "exoclick.com", "juicyads.com", "trafficjunky.net", "adsterra.com", "hilltopads.net",
    "yieldlove.com", "adition.com", "stickyadstv.com", "lkqd.net", "springserve.com",
    "id5-sync.com", "liveramp.com", "liadm.com", "crwdcntrl.net", "addthis.com", "sharethis.com",
    "onesignal.com", "pushcrew.com", "imrworldwide.com", "comscore.com", "dotomi.com",
    "emxdgt.com", "adhigh.net", "yieldoptimizer.com", "admixer.net", "adkernel.com",
];

/// Query parameters used purely for cross-site tracking.
const TRACKING_PARAMS: &[&str] = &[
    "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid", "msclkid", "mc_cid", "mc_eid",
    "yclid", "_openstat", "igshid", "twclid", "ttclid", "li_fat_id", "_hsenc", "_hsmi",
    "mkt_tok", "oly_anon_id", "oly_enc_id", "vero_id", "rb_clickid", "s_cid", "__s",
];

/// Extracts the lowercase host from an http(s)/ws(s) URL.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = if host_port.starts_with('[') {
        host_port.split(']').next()?.trim_start_matches('[')
    } else {
        host_port.split(':').next()?
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

pub fn is_blocked_host(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    BLOCKED_DOMAINS.iter().any(|d| {
        host == *d || (host.len() > d.len() && host.ends_with(d) && host.as_bytes()[host.len() - d.len() - 1] == b'.')
    })
}

pub fn is_blocked_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    if !(lower.starts_with("http") || lower.starts_with("ws")) {
        return false;
    }
    host_of(url).is_some_and(|h| is_blocked_host(&h))
}

fn is_local_host(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host == "127.0.0.1" || host == "::1"
        || host.starts_with("192.168.") || host.starts_with("10.")
}

/// Returns the https:// form of a plain-http URL to a public host.
pub fn https_upgrade(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let host = host_of(url)?;
    if is_local_host(&host) {
        return None;
    }
    Some(format!("https://{rest}"))
}

fn is_tracking_param(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("utm_") || TRACKING_PARAMS.contains(&n.as_str())
}

/// Removes tracking query parameters. Returns `None` if nothing was removed.
pub fn strip_tracking_params(url: &str) -> Option<String> {
    let (before_frag, frag) = match url.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (url, None),
    };
    let (base, query) = before_frag.split_once('?')?;
    let kept: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty() && !is_tracking_param(p.split('=').next().unwrap_or("")))
        .collect();
    if kept.len() == query.split('&').filter(|p| !p.is_empty()).count() {
        return None;
    }
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    Some(out)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Turns address-bar input into a URL: a web address, or a search using `search_prefix`.
pub fn resolve_input(input: &str, search_prefix: &str) -> String {
    let t = input.trim();
    let lower = t.to_ascii_lowercase();
    if ["https://", "http://", "about:", "veil://", "view-source:", "file:///"].iter().any(|p| lower.starts_with(p)) {
        return t.to_string();
    }
    let looks_like_host = !t.contains(char::is_whitespace)
        && (t.contains('.') || lower.starts_with("localhost"))
        && !t.starts_with('.')
        && !t.ends_with('.');
    if looks_like_host {
        let scheme = match host_of(&format!("http://{t}")) {
            Some(h) if is_local_host(&h) => "http",
            _ => "https",
        };
        format!("{scheme}://{t}")
    } else {
        format!("{search_prefix}{}", percent_encode(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_domains_and_subdomains_only() {
        assert!(is_blocked_url("https://www.google-analytics.com/collect?v=1"));
        assert!(is_blocked_url("https://securepubads.g.doubleclick.net/tag"));
        assert!(!is_blocked_url("https://notdoubleclick.net/"));
        assert!(!is_blocked_url("https://example.com/doubleclick.net"));
        assert!(!is_blocked_url("data:text/html,doubleclick.net"));
    }

    #[test]
    fn host_parsing() {
        assert_eq!(host_of("https://user@Example.COM:8080/x").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://[::1]:3000/").as_deref(), Some("::1"));
    }

    #[test]
    fn upgrades_http() {
        assert_eq!(https_upgrade("http://example.com/a").as_deref(), Some("https://example.com/a"));
        assert_eq!(https_upgrade("http://localhost:3000/"), None);
        assert_eq!(https_upgrade("https://example.com/"), None);
    }

    #[test]
    fn strips_tracking() {
        assert_eq!(
            strip_tracking_params("https://a.com/p?id=3&utm_source=x&fbclid=y#top").as_deref(),
            Some("https://a.com/p?id=3#top")
        );
        assert_eq!(strip_tracking_params("https://a.com/p?gclid=1").as_deref(), Some("https://a.com/p"));
        assert_eq!(strip_tracking_params("https://a.com/p?id=3"), None);
    }

    #[test]
    fn resolves_input() {
        let ddg = "https://duckduckgo.com/?q=";
        assert_eq!(resolve_input("example.com", ddg), "https://example.com");
        assert_eq!(resolve_input("localhost:8080", ddg), "http://localhost:8080");
        assert_eq!(resolve_input("rust lang", ddg), "https://duckduckgo.com/?q=rust+lang");
        assert_eq!(resolve_input("c++", ddg), "https://duckduckgo.com/?q=c%2B%2B");
        assert_eq!(resolve_input("veil://history", ddg), "veil://history");
    }
}
