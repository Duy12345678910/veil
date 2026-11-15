// Internal pages, part 1: shared helpers, History, Bookmarks, Downloads, Profiles, Help.
const $ = (id) => document.getElementById(id);
const main = $("main");
const R = {};          // renderers by "what"
const cache = {};      // last data by "what"
const NAV = [["history", "clock", "History"], ["bookmarks", "book", "Bookmarks"], ["downloads", "download", "Downloads"],
  ["passwords", "key", "Passwords"], ["settings", "gear", "Settings"], ["extensions", "puzzle", "Extensions and Themes"],
  ["profiles", "user", "Profiles"], ["help", "help", "Help and Shortcuts"]];

$("nav").insertAdjacentHTML("beforeend", NAV.map(([p, i, t]) =>
  `<button data-p="${p}" class="${p === PAGE ? "on" : ""}">${icon(i)}${t}</button>`).join(""));
$("nav").querySelectorAll("[data-p]").forEach((b) => b.onclick = () => send("navigate", { url: "veil://" + b.dataset.p }));

function toast(msg) {
  const t = $("toast"); t.textContent = msg; t.className = "on";
  clearTimeout(toast.t); toast.t = setTimeout(() => t.className = "", 3200);
}
const open = (url, e) => send("navigate", { url, new_tab: !!(e && (e.ctrlKey || e.button === 1)) });
const letter = (t, u) => { const h = hostOf(u); return `<span class="ltr" style="background:hsl(${hue(h)} 45% 45%)">${esc((t || h || "?")[0].toUpperCase())}</span>`; };
const fmtTime = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
const fmtDay = (ms) => {
  const d = new Date(ms), today = new Date(); const y = new Date(); y.setDate(today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return "Today";
  if (d.toDateString() === y.toDateString()) return "Yesterday";
  return d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", year: "numeric" });
};
function bind(root = main) {
  root.querySelectorAll("a[data-url]").forEach((a) => {
    a.onclick = (e) => { e.preventDefault(); open(a.dataset.url, e); };
    a.onauxclick = (e) => { if (e.button === 1) open(a.dataset.url, e); };
  });
}
function filterBox(id, placeholder, onInput) {
  setTimeout(() => { const el = $(id); if (el) el.oninput = () => onInput(el.value.toLowerCase()); }, 0);
  return `<input class="t grow" id="${id}" placeholder="${placeholder}">`;
}

// ---------- History ----------
let histQ = "";
R.history = (list) => {
  const items = list.filter((h) => !histQ || (h.title + " " + h.url).toLowerCase().includes(histQ));
  let html = `<h1>History</h1><div class="tools">${filterBox("hq", "Search history", (q) => { histQ = q; R.history(cache.history); })}
    <button class="b danger" id="clr">${icon("trash")}Clear all history</button></div>`;
  if (!items.length) html += `<div class="card empty">${list.length ? "No matches." : "Pages you visit will show up here. Private windows are never recorded."}</div>`;
  let day = null;
  for (const h of items.slice(0, 1500)) {
    const d = fmtDay(h.time);
    if (d !== day) { html += (day ? "</div>" : "") + `<div class="day">${d}</div><div class="card">`; day = d; }
    html += `<div class="row"><span class="time">${fmtTime(h.time)}</span>${letter(h.title, h.url)}<div class="main">
      <div class="ttl"><a data-url="${esc(h.url)}" href="#">${esc(h.title || h.url)}</a></div><div class="sub">${esc(h.url)}</div></div>
      <button class="ib" data-del="${h.time}" title="Remove from history">${icon("close")}</button></div>`;
  }
  if (day) html += "</div>";
  const focus = document.activeElement && document.activeElement.id === "hq";
  main.innerHTML = html; bind();
  $("hq").value = histQ; if (focus) $("hq").focus();
  $("clr").onclick = () => { if (confirm("Clear all browsing history for this profile?")) send("clear_history"); };
  main.querySelectorAll("[data-del]").forEach((b) => b.onclick = () => send("delete_history", { time: +b.dataset.del }));
};

// ---------- Bookmarks ----------
let bmQ = "";
R.bookmarks = (list) => {
  const items = list.filter((b) => !bmQ || (b.title + " " + b.url).toLowerCase().includes(bmQ));
  let html = `<h1>Bookmarks</h1><div class="card p"><div class="tools" style="margin:0">
      <input class="t" id="nt" placeholder="Name" style="width:30%"><input class="t grow" id="nu" placeholder="Address">
      <button class="b pri" id="add">${icon("plus")}Add</button></div></div>
    <div class="tools">${filterBox("bq", "Search bookmarks", (q) => { bmQ = q; R.bookmarks(cache.bookmarks); })}
      <button class="b" id="imp">${icon("download")}Import HTML…</button><button class="b" id="exp">${icon("save")}Export HTML…</button></div><div class="card">`;
  if (!items.length) html += `<div class="empty">${list.length ? "No matches." : "No bookmarks yet. Press Ctrl+D on any page, or import from Chrome, Edge or Firefox (Export bookmarks to HTML there)."}</div>`;
  for (const b of items) {
    html += `<div class="row">${letter(b.title, b.url)}<div class="main"><div class="ttl"><a data-url="${esc(b.url)}" href="#">${esc(b.title || b.url)}</a></div>
      <div class="sub">${esc(b.url)}</div></div><button class="ib" data-edit="${b.id}" title="Edit">${icon("edit")}</button>
      <button class="ib" data-del="${b.id}" title="Delete">${icon("trash")}</button></div>`;
  }
  const focus = document.activeElement && document.activeElement.id === "bq";
  main.innerHTML = html + "</div>"; bind();
  $("bq").value = bmQ; if (focus) $("bq").focus();
  $("add").onclick = () => $("nu").value.trim() && send("add_bookmark", { title: $("nt").value.trim(), url: $("nu").value.trim() });
  $("imp").onclick = () => send("import_bookmarks"); $("exp").onclick = () => send("export_bookmarks");
  main.querySelectorAll("[data-del]").forEach((b) => b.onclick = () => send("delete_bookmark", { id: +b.dataset.del }));
  main.querySelectorAll("[data-edit]").forEach((b) => b.onclick = () => {
    const bm = list.find((x) => x.id === +b.dataset.edit);
    const title = prompt("Bookmark name", bm.title); if (title === null) return;
    const url = prompt("Address", bm.url); if (url === null) return;
    send("edit_bookmark", { id: bm.id, title, url });
  });
};

// ---------- Downloads ----------
R.downloads = (list) => {
  let html = `<h1>Downloads</h1><div class="tools"><span class="note" style="flex:1">Files are saved to your Downloads folder.</span>
    <button class="b" id="clr">${icon("trash")}Clear list</button></div><div class="card">`;
  if (!list.length) html += `<div class="empty">Files you download will appear here.</div>`;
  for (const d of list) {
    const name = d.path.split(/[\\/]/).pop() || d.url;
    const st = d.state === "active" ? '<span class="badge">Downloading…</span>' : d.state === "failed" ? '<span class="badge" style="color:var(--danger)">Failed</span>' : "";
    html += `<div class="row">${icon("download")}<div class="main"><div class="ttl">${esc(name)} ${st}</div><div class="sub">${esc(hostOf(d.url))} · ${fmtDay(d.time)} ${fmtTime(d.time)}</div></div>
      ${d.state === "done" ? `<button class="b sm" data-open="${d.id}">Open</button><button class="ib" data-folder="${d.id}" title="Show in folder">${icon("folder")}</button>` : ""}
      <button class="ib" data-del="${d.id}" title="Remove from list">${icon("close")}</button></div>`;
  }
  main.innerHTML = html + "</div>";
  $("clr").onclick = () => send("clear_downloads");
  main.querySelectorAll("[data-open]").forEach((b) => b.onclick = () => send("open_download", { id: +b.dataset.open }));
  main.querySelectorAll("[data-folder]").forEach((b) => b.onclick = () => send("open_download", { id: +b.dataset.folder, folder: true }));
  main.querySelectorAll("[data-del]").forEach((b) => b.onclick = () => send("delete_download", { id: +b.dataset.del }));
};

// ---------- Profiles ----------
R.profiles = (d) => {
  let html = `<h1>Profiles</h1><p class="note" style="margin-bottom:16px">Each profile is a separate user with its own history, bookmarks, passwords, cookies, extensions and settings.</p>
    <div class="card p"><div class="tools" style="margin:0"><input class="t grow" id="pn" placeholder="New profile name (e.g. Work, School)" maxlength="40">
    <button class="b pri" id="pc">${icon("plus")}Create profile</button></div></div><div class="card">`;
  for (const p of d.list) {
    html += `<div class="row"><span class="ltr" style="background:${esc(p.color)};border-radius:50%">${esc(p.name[0].toUpperCase())}</span>
      <div class="main"><div class="ttl">${esc(p.name)} ${p.slug === d.current ? '<span class="badge">This window</span>' : ""}</div></div>
      <button class="b sm" data-open="${esc(p.slug)}">${icon("window")}Open window</button>
      <button class="b sm" data-priv="${esc(p.slug)}">${icon("mask")}Private</button>
      ${p.slug === "default" ? "" : `<button class="ib" data-del="${esc(p.slug)}" title="Delete profile">${icon("trash")}</button>`}</div>`;
  }
  main.innerHTML = html + "</div>";
  $("pc").onclick = () => { send("create_profile", { name: $("pn").value }); $("pn").value = ""; };
  main.querySelectorAll("[data-open]").forEach((b) => b.onclick = () => send("open_profile", { slug: b.dataset.open }));
  main.querySelectorAll("[data-priv]").forEach((b) => b.onclick = () => send("open_profile", { slug: b.dataset.priv, private: true }));
  main.querySelectorAll("[data-del]").forEach((b) => b.onclick = () => {
    if (confirm("Delete this profile and ALL its data (history, bookmarks, passwords, cookies)? This can't be undone.")) send("delete_profile", { slug: b.dataset.del });
  });
};

// ---------- Help ----------
R.help = (shortcuts) => {
  main.innerHTML = `<h1>Help and Shortcuts</h1><div class="card p about">{{LOGO}}<div><h2 style="margin:0 0 6px">Veil By Claude, Built in Rust.</h2>
    <div class="note">Version 0.2.0 · Rendering engine: Microsoft Edge WebView2<br>Built-in tracker &amp; ad blocking, HTTPS-only mode, tracking-link cleaning,
    Global Privacy Control, encrypted password storage, private windows and separate profiles.</div></div></div>
    <h2>Keyboard shortcuts</h2><div class="card"><table>${shortcuts.map(([k, d]) =>
      `<tr><td>${k.split(" / ").map((x) => x.split("+").map((p) => `<kbd>${esc(p)}</kbd>`).join("+")).join(" or ")}</td><td>${esc(d)}</td></tr>`).join("")}</table></div>`;
};
