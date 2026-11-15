// Internal pages, part 2: Passwords, Settings (privacy, cookies, clearing data), Extensions and Themes.

function copyText(text) {
  const ta = document.createElement("textarea");
  ta.value = text; document.body.appendChild(ta); ta.select();
  document.execCommand("copy"); ta.remove(); toast("Copied to clipboard");
}

// ---------- Passwords ----------
let pwQ = "";
const shown = new Set();
R.passwords = (list) => {
  const items = list.filter((l) => !pwQ || (l.origin + " " + l.username).toLowerCase().includes(pwQ));
  let html = `<h1>Passwords</h1><div class="card p note" style="display:flex;gap:12px;align-items:center">${icon("lock")}
      <span>Saved logins are encrypted with your Windows account (DPAPI) and never leave this PC. Click the ${icon("key")} key in the address bar to fill a login.</span></div>
    <div class="card p"><b>Add a login</b><div class="tools" style="margin:10px 0 0">
      <input class="t" id="po" placeholder="Website (example.com)" style="width:32%"><input class="t" id="pu" placeholder="Username" style="width:26%">
      <input class="t grow" id="pp" type="password" placeholder="Password"><button class="b pri" id="pa">${icon("plus")}Save</button></div></div>
    <div class="tools">${filterBox("pq", "Search passwords", (q) => { pwQ = q; R.passwords(cache.passwords); })}
      <button class="b" id="imp">${icon("download")}Import CSV…</button><button class="b" id="exp">${icon("save")}Export CSV…</button></div><div class="card">`;
  if (!items.length) html += `<div class="empty">${list.length ? "No matches." : "No saved passwords. Import a CSV exported from Chrome, Edge or Firefox (Settings → Passwords → Export), or add one above."}</div>`;
  for (const l of items) {
    const vis = shown.has(l.id);
    html += `<div class="row">${letter(hostOf(l.origin), l.origin)}<div class="main"><div class="ttl">${esc(hostOf(l.origin))}</div><div class="sub">${esc(l.origin)}</div></div>
      <div class="main"><div class="ttl">${esc(l.username || "—")}</div><div class="sub pw">${vis ? esc(l.password) : "••••••••••"}</div></div>
      <button class="ib" data-cu="${l.id}" title="Copy username">${icon("user")}</button>
      <button class="ib" data-cp="${l.id}" title="Copy password">${icon("copy")}</button>
      <button class="ib" data-eye="${l.id}" title="${vis ? "Hide" : "Show"} password">${icon("eye")}</button>
      <button class="ib" data-del="${l.id}" title="Delete">${icon("trash")}</button></div>`;
  }
  const focus = document.activeElement && document.activeElement.id === "pq";
  main.innerHTML = html + "</div>";
  $("pq").value = pwQ; if (focus) $("pq").focus();
  const find = (id) => list.find((l) => l.id === +id);
  $("pa").onclick = () => {
    if (!$("po").value.trim() || !$("pp").value) return toast("Enter a website and a password");
    send("add_password", { origin: $("po").value, username: $("pu").value, password: $("pp").value });
  };
  $("imp").onclick = () => send("import_passwords");
  $("exp").onclick = () => { if (confirm("The exported CSV file will contain your passwords in plain text. Continue?")) send("export_passwords"); };
  main.querySelectorAll("[data-cu]").forEach((b) => b.onclick = () => copyText(find(b.dataset.cu).username));
  main.querySelectorAll("[data-cp]").forEach((b) => b.onclick = () => copyText(find(b.dataset.cp).password));
  main.querySelectorAll("[data-eye]").forEach((b) => b.onclick = () => { const id = +b.dataset.eye; shown.has(id) ? shown.delete(id) : shown.add(id); R.passwords(cache.passwords); });
  main.querySelectorAll("[data-del]").forEach((b) => b.onclick = () => { if (confirm("Delete this saved login?")) send("delete_password", { id: +b.dataset.del }); });
};

// ---------- Settings ----------
const TOGGLES = [
  ["block_trackers", "Block trackers and ads", "Stops requests to ~190 known advertising and tracking networks."],
  ["strict_tracking_prevention", "Strict tracking prevention", "Uses the engine's strict mode to block trackers' storage and fingerprinting."],
  ["block_third_party_cookies", "Block third-party cookies", "Sites can't set cookies for other companies. Applies after restarting Veil."],
  ["https_only", "HTTPS-only mode", "Always upgrades insecure http:// pages to https://."],
  ["strip_tracking", "Remove tracking from links", "Strips utm_*, fbclid, gclid, msclkid and similar tags from addresses."],
  ["send_gpc", "Tell websites not to sell or share my data", "Sends Global Privacy Control and Do Not Track signals."],
  ["save_history", "Remember browsing history", "Private windows never save history."],
  ["clear_on_exit", "Clear cookies, cache and history when Veil closes", "Like a fresh start every time."],
];
const set = (key, value) => send("set_setting", { key, value });
R.settings = (d) => {
  const s = d.settings;
  main.innerHTML = `<h1>Settings</h1>
    ${d.private ? '<div class="card p note">You are in a private window. Changes here apply to the profile.</div>' : ""}
    <h2>General</h2><div class="card">
      <div class="set"><div class="main" style="flex:1"><b>Search engine</b><span>Used in the address bar and on the new tab page.</span></div>
        <select id="eng">${d.engines.map(([id, n]) => `<option value="${id}" ${id === s.search_engine ? "selected" : ""}>${n}</option>`).join("")}</select></div>
      <div class="set"><div class="main" style="flex:1"><b>Theme</b><span>More in Extensions and Themes.</span></div>
        <select id="thm">${d.themes.map(([id, n]) => `<option value="${id}" ${id === s.theme ? "selected" : ""}>${n}</option>`).join("")}</select></div>
      <div class="set"><div class="main" style="flex:1"><b>Show bookmarks bar</b><span>Ctrl+Shift+B</span></div><button class="sw ${s.show_bookmarks_bar ? "on" : ""}" data-k="show_bookmarks_bar"></button></div>
    </div>
    <h2 id="privacy">Privacy and security</h2><div class="card">${TOGGLES.map(([k, t, sub]) =>
      `<div class="set"><div class="main" style="flex:1"><b>${t}</b><span>${sub}</span></div><button class="sw ${s[k] ? "on" : ""}" data-k="${k}"></button></div>`).join("")}</div>
    <h2>Clear browsing data</h2><div class="card p">
      <div class="tools">${[["c_hist", "Browsing history", 1], ["c_cook", "Cookies and site data", 1], ["c_cache", "Cached images and files", 1], ["c_dl", "Download list", 0]]
        .map(([id, t, on]) => `<label style="display:flex;gap:6px;align-items:center;margin-right:14px"><input type="checkbox" id="${id}" ${on ? "checked" : ""}>${t}</label>`).join("")}</div>
      <button class="b danger" id="clear">${icon("trash")}Clear data now</button></div>
    <h2>Cookies and site data</h2><div class="card" id="cookies"><div class="empty">Loading cookies…</div></div>
    <h2>Passwords</h2><div class="card p tools" style="margin:0"><span class="note" style="flex:1">Import CSV passwords, view and manage saved logins.</span>
      <button class="b" onclick="send('navigate',{url:'veil://passwords'})">${icon("key")}Manage passwords</button></div>`;
  $("eng").onchange = (e) => set("search_engine", e.target.value);
  $("thm").onchange = (e) => set("theme", e.target.value);
  main.querySelectorAll(".sw").forEach((b) => b.onclick = () => set(b.dataset.k, !b.classList.contains("on")));
  $("clear").onclick = () => {
    const o = { history: $("c_hist").checked, cookies: $("c_cook").checked, cache: $("c_cache").checked, downloads: $("c_dl").checked };
    if (confirm("Clear the selected data for this profile?")) send("clear_data", o);
  };
  if (cache.cookies) R.cookies(cache.cookies); else send("get", { what: "cookies" });
};
R.cookies = (list) => {
  const box = $("cookies"); if (!box) return;
  const by = {};
  for (const c of list) by[c.domain] = (by[c.domain] || 0) + 1;
  const sites = Object.entries(by).sort((a, b) => b[1] - a[1]);
  box.innerHTML = `<div class="row" style="background:none"><span class="note" style="flex:1">${list.length} cookies from ${sites.length} sites</span>
    <button class="b sm" id="crf">${icon("reload")}Refresh</button></div>` + (sites.length ? sites.map(([d, n]) =>
    `<div class="row">${icon("cookie")}<div class="main"><div class="ttl">${esc(d)}</div><div class="sub">${n} cookie${n > 1 ? "s" : ""}</div></div>
     <button class="b sm danger" data-dom="${esc(d)}">Delete</button></div>`).join("") : '<div class="empty">No cookies stored.</div>');
  $("crf").onclick = () => send("cookies", {});
  box.querySelectorAll("[data-dom]").forEach((b) => b.onclick = () => send("cookies", { delete: b.dataset.dom }));
};

// ---------- Extensions and Themes ----------
const BUILTIN = ["mhjfbmdgcfjbbpaeojofohoefgiehjai", "dgiklkfkllikcanfonkcabmbdfmgleag"]; // engine's PDF viewer & clipboard helper
const THEME_PREVIEW = { "claude-dark": ["#1b1816", "#2a2522", "#d97757"], "claude-light": ["#e8e2d6", "#faf8f3", "#c2603b"],
  "ember": ["#24110b", "#3a1b11", "#ff8a5c"], "midnight": ["#10121c", "#1b1f2e", "#d97757"] };
R.extensions = (d) => {
  if (d.settings) { cache.settings = d; return R.extensions(cache.extensions || { list: [] }); }
  const theme = cache.settings ? cache.settings.settings.theme : "claude-dark";
  const themes = cache.settings ? cache.settings.themes : [];
  main.innerHTML = `<h1>Extensions and Themes</h1><h2>Themes</h2><div class="themes">${themes.map(([id, n]) => {
      const [a, b, c] = THEME_PREVIEW[id] || ["#222", "#333", "#d97757"];
      return `<button class="theme ${id === theme ? "on" : ""}" data-t="${id}"><div class="pv" style="background:${a}"><div style="height:24px;background:${a}"></div>
        <div style="flex:1;background:${b};display:flex;align-items:center;padding:0 10px"><div style="height:10px;width:60%;border-radius:5px;background:${c}"></div></div></div>
        <div class="nm">${n}</div></button>`; }).join("")}</div>
    <h2>Extensions</h2><div class="card p note">Load an <b>unpacked</b> Chrome/Edge extension folder (one that contains a manifest.json). Content-blocking and
      page-modifying extensions work. Toolbar buttons and pop-ups aren't supported by the WebView2 engine. Extensions aren't available in private windows.</div>
    <div class="tools"><button class="b pri" id="addx">${icon("puzzle")}Load unpacked extension…</button></div>
    <div class="card">${d.error ? `<div class="empty">${esc(d.error)}</div>` : (d.list || []).length ? d.list.map((x) =>
      `<div class="row">${icon("puzzle")}<div class="main"><div class="ttl">${esc(x.name)}</div><div class="sub">${esc(x.id)}</div></div>
       ${BUILTIN.includes(x.id) ? '<span class="badge">Built-in</span>' : `<button class="sw ${x.enabled ? "on" : ""}" data-tog="${esc(x.id)}" title="Enable or disable"></button>
       <button class="ib" data-rm="${esc(x.id)}" title="Remove">${icon("trash")}</button>`}</div>`).join("") : '<div class="empty">No extensions installed.</div>'}</div>`;
  main.querySelectorAll("[data-t]").forEach((b) => b.onclick = () => set("theme", b.dataset.t));
  $("addx").onclick = () => send("add_extension");
  main.querySelectorAll("[data-tog]").forEach((b) => b.onclick = () => send("toggle_extension", { id: b.dataset.tog, enabled: !b.classList.contains("on") }));
  main.querySelectorAll("[data-rm]").forEach((b) => b.onclick = () => { if (confirm("Remove this extension?")) send("remove_extension", { id: b.dataset.rm }); });
};

window.veil = {
  recv(what, data) {
    if (what === "toast") return toast(data);
    if (what === "scroll") return setTimeout(() => $(data)?.scrollIntoView({ behavior: "smooth" }), 150);
    if (what === "cleared") { cache.cookies = null; toast(data.ok ? "Browsing data cleared" : "Couldn't clear everything"); return send("get", { what: PAGE === "settings" ? "cookies" : PAGE }); }
    if (what === "extension_added") { toast(data.error || "Extension loaded"); return send("get", { what: "extensions" }); }
    if (what === "settings" && PAGE === "extensions") { cache.settings = data; return R.extensions(cache.extensions || { list: [] }); }
    cache[what] = data;
    if (what === "cookies") return R.cookies(data);
    if (R[what]) R[what](data);
  },
};
if (PAGE === "extensions") { send("get", { what: "settings" }); }
send("get", { what: PAGE });
