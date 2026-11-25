// PC Dashboard - frontend
const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
const win = T.window.getCurrentWindow();
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const icon = (id) => `<svg><use href="#${id}"/></svg>`;

function gb(bytes) {
  if (!bytes) return "0 GB";
  const g = bytes / 1073741824;
  if (g < 1) return Math.max(1, Math.round(bytes / 1048576)).toLocaleString(LOC) + " MB";
  return g.toLocaleString(LOC, { maximumFractionDigits: g < 10 ? 1 : 0 }) + " GB";
}
function since(sec) {
  const d = Math.floor(sec / 86400), h = Math.floor(sec / 3600) % 24, m = Math.floor(sec / 60) % 60;
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}
function greeting() {
  const h = new Date().getHours();
  if (LANG === "de") {
    const g = h < 5 ? "Noch wach" : h < 11 ? "Guten Morgen" : h < 18 ? "Guten Tag" : "Guten Abend";
    return `${g} · ${new Date().toLocaleDateString(LOC, { weekday: "long", month: "long", day: "numeric" })}`;
  }
  const g = h < 5 ? "Up late" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
  return `${g} · ${new Date().toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" })}`;
}

/* ---------- window ---------- */
$("#w-min").onclick = () => win.minimize();
$("#w-close").onclick = () => win.close();
/* right-click: hide a tab or Jarvis, bring everything back */
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  const tab = e.target.closest(".nav")?.dataset.page || (e.target.closest("#jarvis") ? "jarvis" : null);
  const items = [];
  if (tab && tab !== "overview" && tab !== "settings") items.push(["i-eye", "Hide", () => hideTab(tab, true)]);
  if (e.target.closest(".side") && cs?.hidden.length) items.push(["i-check", "Show all tabs", () => clipSet({ hidden: [] })]);
  const m = $("#ctx");
  if (!items.length) return m.classList.remove("open");
  m.innerHTML = items.map(([i, t]) => `<button>${icon(i)}<span>${t}</span></button>`).join("");
  m.querySelectorAll("button").forEach((b, i) => (b.onclick = () => { m.classList.remove("open"); items[i][2](); }));
  m.style.left = Math.min(e.clientX, innerWidth - 190) + "px";
  m.style.top = Math.min(e.clientY, innerHeight - 100) + "px";
  m.classList.add("open");
});
document.addEventListener("pointerdown", (e) => { if (!e.target.closest("#ctx")) $("#ctx").classList.remove("open"); });
window.addEventListener("blur", () => $("#ctx").classList.remove("open"));
function hideTab(id, hide) {
  const set = new Set(cs.hidden);
  hide ? set.add(id) : set.delete(id);
  return clipSet({ hidden: [...set] });
}
function applyHidden(list) {
  document.querySelectorAll(".nav").forEach((b) => (b.hidden = list.includes(b.dataset.page)));
  $("#jarvis").hidden = list.includes("jarvis");
  document.querySelector(".top-row").classList.toggle("solo", list.includes("jarvis"));
  document.querySelectorAll(".tile").forEach((t) => {
    const alt = t.dataset.alt;           // takes the place of another tile when that one is hidden
    t.hidden = list.includes(t.dataset.go) || (alt && !list.includes(alt));
  });
  document.querySelector(".tiles").hidden = ![...document.querySelectorAll(".tile")].some((t) => !t.hidden);
  if (list.includes(current)) go("overview"); else moveIndicator();
}

/* ---------- light that follows the mouse over cards ---------- */
document.addEventListener("mousemove", (e) => {
  for (const c of document.querySelectorAll(".page.show .card")) {
    const r = c.getBoundingClientRect();
    c.style.setProperty("--mx", e.clientX - r.left + "px");
    c.style.setProperty("--my", e.clientY - r.top + "px");
  }
});

/* ---------- navigation ---------- */
const PAGES = {
  overview: ["Overview", greeting],
  clips: ["Clips", () => "Your whole screen, saved whenever something good happens"],
  storage: ["Storage", () => "Clean up and see what eats your space"],
  autostart: ["Startup apps", () => "Everything that launches when Windows starts"],
  themes: ["Themes", () => "Desktop themes found on this PC"],
  settings: ["Settings", () => "Make the app yours"],
};
let current = "overview";
function moveIndicator() {
  const a = document.querySelector(".nav.active");
  $("#nav-ind").style.transform = `translateY(${a.offsetTop}px)`;
}
function go(page) {
  current = page;
  document.querySelectorAll(".nav").forEach((b) => b.classList.toggle("active", b.dataset.page === page));
  document.querySelectorAll(".page").forEach((p) => p.classList.toggle("show", p.id === page));
  $("#title").textContent = PAGES[page][0];
  $("#subtitle").textContent = PAGES[page][1]();
  $("#scroll").scrollTop = 0;
  moveIndicator();
  // always show the current state, things change outside the app too
  if (page === "overview") loadDisk().catch(console.error);
  if (page === "autostart") loadAutostart().catch(console.error);
  if (page === "settings") loadSettings();
  if (page === "clips") { clipState(); loadClips().catch(console.error); }
}
document.querySelectorAll(".nav").forEach((b) => (b.onclick = () => go(b.dataset.page)));
document.querySelectorAll("[data-go]").forEach((b) => (b.onclick = () => go(b.dataset.go)));

/* ---------- toast + modal ---------- */
function toast(text, err = false) {
  const t = document.createElement("div");
  t.className = "toast" + (err ? " err" : "");
  t.innerHTML = icon(err ? "i-close" : "i-check") + `<span>${esc(text)}</span>`;
  $("#toasts").append(t);
  setTimeout(() => { t.classList.add("out"); setTimeout(() => t.remove(), 300); }, err ? 6000 : 3500);
}
function confirmBox(title, text, rows) {
  return new Promise((done) => {
    $("#m-title").textContent = title;
    $("#m-text").textContent = text;
    $("#m-list").innerHTML = rows.map(([a, b]) => `<div><span>${esc(a)}</span><b>${esc(b)}</b></div>`).join("");
    $("#modal").classList.add("open");
    const close = (v) => { $("#modal").classList.remove("open"); done(v); };
    $("#m-yes").onclick = () => close(true);
    $("#m-no").onclick = () => close(false);
    $("#modal").onclick = (e) => { if (e.target.id === "modal") close(false); };
  });
}

/* ---------- drives: one page per drive, swipe or use the dots ---------- */
const drv = { list: [], at: 0 };
try { drv.want = localStorage.getItem("drive"); } catch {}
async function loadDisk() { renderDisks(await invoke("disks")); }
function renderDisks(list) {
  const keep = drv.list.length ? drv.list[drv.at]?.letter : drv.want;
  const same = list.map((d) => d.letter).join() === drv.list.map((d) => d.letter).join();
  drv.list = list;
  if (!same) {
    $("#d-track").innerHTML = list.map((d, i) => `
      <div class="d-slide">
        <div class="s-top"><svg><use href="#i-disk"/></svg><span>Drive ${d.letter}:</span><span class="d-name"></span></div>
        <div class="ring">
          <svg viewBox="0 0 160 160">
            <defs><mask id="ringmask${i}"><circle class="off" cx="80" cy="80" r="66" stroke="#fff" stroke-width="12" fill="none" stroke-linecap="round" stroke-dasharray="414.7" stroke-dashoffset="414.7"/></mask></defs>
            <circle class="track" cx="80" cy="80" r="66"/>
            <circle class="fill off" cx="80" cy="80" r="66" style="mask:url(#ringmask${i})"/>
          </svg>
          <div class="ring-in"><b class="free">–</b><span>free</span></div>
        </div>
        <div class="legend"><span><i class="dot a"></i><b class="used">–</b> used</span><span><i class="dot b"></i><b class="total">–</b> total</span></div>
      </div>`).join("");
    $("#d-dots").innerHTML = list.length > 1 ? list.map((d) => `<button title="${d.letter}:" aria-label="${d.letter}:"><i></i></button>`).join("") : "";
    $("#drive").classList.toggle("many", list.length > 1);
    document.querySelectorAll("#d-dots button").forEach((b, i) => (b.onclick = () => showDrive(i)));
    showDrive(list.findIndex((d) => d.letter === keep), false);
  }
  document.querySelectorAll(".d-slide").forEach((s, i) => {
    const d = list[i], used = d.total - d.free;
    s.querySelector(".d-name").textContent = d.name;
    s.querySelector(".free").textContent = gb(d.free);
    s.querySelector(".free").classList.toggle("long", gb(d.free).length > 7);
    s.querySelector(".used").textContent = gb(used);
    s.querySelector(".total").textContent = gb(d.total);
    requestAnimationFrame(() => s.querySelectorAll(".off").forEach((c) => (c.style.strokeDashoffset = 414.7 * (1 - used / d.total))));
  });
}
function showDrive(i, save = true) {
  drv.at = Math.max(0, Math.min(drv.list.length - 1, i));
  $("#d-track").style.transform = `translateX(${-100 * drv.at}%)`;
  document.querySelectorAll("#d-dots button").forEach((b, n) => b.classList.toggle("on", n === drv.at));
  if (save) try { localStorage.setItem("drive", drv.list[drv.at].letter); } catch {}
}
{
  const card = $("#drive"), track = $("#d-track");
  let x0 = null, dx = 0, wheelAt = -1000;
  card.addEventListener("pointerdown", (e) => { if (drv.list.length < 2 || e.button || e.target.closest("button")) return; x0 = e.clientX; dx = 0; try { card.setPointerCapture(e.pointerId); } catch {} track.classList.add("drag"); });
  card.addEventListener("pointermove", (e) => {
    if (x0 == null) return;
    dx = e.clientX - x0;
    const end = (drv.at === 0 && dx > 0) || (drv.at === drv.list.length - 1 && dx < 0);      // rubber band at the ends
    track.style.transform = `translateX(calc(${-100 * drv.at}% + ${end ? dx * 0.25 : dx}px))`;
  });
  const drop = () => { if (x0 == null) return; x0 = null; track.classList.remove("drag"); showDrive(drv.at + (dx < -45 ? 1 : dx > 45 ? -1 : 0)); };
  card.addEventListener("pointerup", drop);
  card.addEventListener("pointercancel", drop);
  card.addEventListener("wheel", (e) => {
    const s = $("#scroll"), side = Math.abs(e.deltaX) > Math.abs(e.deltaY);
    if (drv.list.length < 2 || (!side && s.scrollHeight > s.clientHeight + 2)) return;      // the page itself scrolls
    e.preventDefault();
    if (e.timeStamp - wheelAt < 350) return;
    wheelAt = e.timeStamp;
    showDrive(drv.at + ((side ? e.deltaX : e.deltaY) > 0 ? 1 : -1));
  }, { passive: false });
}

/* ---------- live stats with sparklines ---------- */
const HIST = 50, hist = { cpu: [], ram: [], gpu: [] };
function spark(el, data) {
  const pts = data.map((v, i) => [(i / (HIST - 1)) * 300, 66 - (v / 100) * 60]);
  const start = HIST - data.length;
  const p = pts.map(([x, y], i) => [((start + i) / (HIST - 1)) * 300, y]);
  if (p.length < 2) return;
  let d = `M${p[0][0]},${p[0][1]}`;
  for (let i = 1; i < p.length; i++) {               // smooth curve
    const [x0, y0] = p[i - 1], [x1, y1] = p[i], cx = (x0 + x1) / 2;
    d += ` C${cx},${y0} ${cx},${y1} ${x1},${y1}`;
  }
  el.querySelector(".line").setAttribute("d", d);
  el.querySelector(".area").setAttribute("d", d + ` L${p[p.length - 1][0]},70 L${p[0][0]},70 Z`);
}
function push(key, v) {
  if (!hist[key].length) hist[key] = Array(HIST - 1).fill(v);   // start as a full-width line
  hist[key].push(v); if (hist[key].length > HIST) hist[key].shift();
}
let hidden = false;      // closed to the tray: no need to measure anything
T.event.listen("hidden", () => { hidden = true; closePlayer(); });
T.event.listen("shown", () => { hidden = false; tick(); if (current === "clips") loadClips().catch(console.error); });
async function tick() {
  if (hidden) return;
  try {
    const s = await invoke("stats");
    $("#uptime").textContent = since(s.uptime);
    if (current === "overview") $("#subtitle").textContent = greeting();
    const ram = (s.ram_used / s.ram_total) * 100;
    push("cpu", s.cpu); push("ram", ram);
    const set = (id, val, sub, key) => {
      const el = $(id);
      el.querySelector(".s-val").textContent = val;
      if (sub != null) el.querySelector(".s-sub").textContent = sub;
      spark(el.querySelector(".spark"), hist[key]);
    };
    set("#s-cpu", Math.round(s.cpu) + "%", s.cpu_name || null, "cpu");
    set("#s-ram", Math.round(ram) + "%", `${gb(s.ram_used)} of ${gb(s.ram_total)}`, "ram");
    if (s.gpu != null) {
      push("gpu", s.gpu);
      set("#s-gpu", Math.round(s.gpu) + "%", `${s.gpu_name} · ${Math.round(s.gpu_temp)}°C · ${gb(s.vram_used)} VRAM`, "gpu");
    } else set("#s-gpu", "–", "No NVIDIA GPU found", "gpu");
  } catch (e) { console.error(e); }
}

/* ---------- cleanup ---------- */
let candidates = [], picked = new Set();
async function loadCleanup() {
  candidates = await invoke("cleanup_candidates");
  picked = new Set(candidates.filter((c) => c.safe).map((c) => c.id));
  const safe = candidates.filter((c) => c.safe).reduce((s, c) => s + c.size, 0);
  const all = candidates.reduce((s, c) => s + c.size, 0);
  $("#t-clean").textContent = gb(all);
  $("#t-clean-s").textContent = !candidates.length ? "all clean" : safe ? `${gb(safe)} of it risk-free` : "only stuff to review";
  renderCleanup();
}
function renderCleanup() {
  const list = $("#clean-list");
  if (!candidates.length) list.innerHTML = `<div class="empty">${icon("i-check")}Nothing to clean. Spotless!</div>`;
  else list.innerHTML = candidates.map((c) => `
    <div class="row pick ${picked.has(c.id) ? "sel" : ""}" data-id="${c.id}">
      <span class="cb">${icon("i-check")}</span>
      <div class="main">
        <div class="name">${esc(c.title)} <span class="badge ${c.safe ? "safe" : "check"}">${c.safe ? "Safe" : "Review"}</span></div>
        <div class="sub">${esc(c.desc)}${c.count > 1 ? ` · ${c.count} items` : ""}</div>
      </div>
      <div class="size">${gb(c.size)}</div>
    </div>`).join("");
  list.querySelectorAll(".row.pick").forEach((r) => (r.onclick = () => {
    picked.has(r.dataset.id) ? picked.delete(r.dataset.id) : picked.add(r.dataset.id);
    renderCleanup();
  }));
  const sel = candidates.filter((c) => picked.has(c.id));
  $("#sel").textContent = gb(sel.reduce((s, c) => s + c.size, 0));
  $("#clean-btn").disabled = !sel.length;
}
$("#clean-btn").onclick = async () => {
  const sel = candidates.filter((c) => picked.has(c.id));
  const ok = await confirmBox("Move to Recycle Bin?", "These will be moved to the Recycle Bin. You can restore them from there.", sel.map((c) => [c.title, gb(c.size)]));
  if (!ok) return;
  const btn = $("#clean-btn");
  btn.disabled = true; btn.innerHTML = `<span class="spin"></span>Cleaning …`;
  try {
    const r = await invoke("clean", { ids: sel.map((c) => c.id) });
    if (r.done.length) toast(`Freed ${gb(r.freed)}`);
    r.failed.forEach((f) => toast(`Partly failed: ${f}`, true));
  } catch (e) { toast(String(e), true); }
  btn.innerHTML = `${icon("i-trash")}Move to Recycle Bin`;
  await Promise.all([loadCleanup(), loadDisk()]);
};

/* ---------- biggest folders ---------- */
$("#scan-btn").onclick = async () => {
  const btn = $("#scan-btn"), list = $("#folder-list");
  btn.disabled = true; btn.innerHTML = `<span class="spin"></span>Scanning …`;
  list.innerHTML = `<div class="empty"><span class="spin"></span>Measuring folders …</div>`;
  const t0 = performance.now();
  const rows = await invoke("scan_folders");
  const max = rows[0]?.size || 1;
  list.innerHTML = rows.map((f, i) => `
    <div class="row">
      <div class="main">
        <div class="name">${esc(f.name)}</div>
        <div class="sub">${esc(f.parent)}</div>
        <div class="fbar"><i style="width:${(f.size / max) * 100}%;animation-delay:${i * 0.04}s"></i></div>
      </div>
      <div class="size">${gb(f.size)}</div>
      <button class="icon-btn" title="Open in Explorer" data-path="${esc(f.path)}">${icon("i-folder")}</button>
    </div>`).join("");
  list.querySelectorAll("[data-path]").forEach((b) => (b.onclick = () => invoke("open_folder", { path: b.dataset.path }).catch((e) => toast(String(e), true))));
  btn.disabled = false; btn.innerHTML = `${icon("i-scan")}Scan again`;
  toast(`Scan done in ${Math.round((performance.now() - t0) / 1000)}s`);
};

/* ---------- startup apps ---------- */
const iconCache = {};
function appIcon(i) {
  const src = iconCache[i.icon];
  return `<div class="app-ic" data-icon="${esc(i.icon)}">${src ? `<img src="${src}" alt="">` : `<span>${esc(i.name.trim()[0]?.toUpperCase() || "?")}</span>`}</div>`;
}
function startRow(i) {
  const path = i.kind === "Store" ? i.command : (i.icon || i.command);
  return `
    <div class="row ${i.enabled && !i.missing ? "" : "off"}">
      ${appIcon(i)}
      <div class="main">
        <div class="name">${esc(i.name)} ${i.missing ? `<span class="badge check">Not installed</span>` : ""}${i.kind !== "App" ? `<span class="badge kind">${esc(i.kind)}</span>` : ""}${i.system ? `<span class="badge sys">System</span>` : ""}</div>
        <div class="sub" title="${esc(i.command)}">${esc(path)}</div>
      </div>
      ${i.missing && !i.system && i.kind !== "Task"
        ? `<button class="icon-btn rm" data-rm="${esc(i.id)}" data-name="${esc(i.name)}" title="Remove this leftover entry">${icon("i-trash")}</button>`
        : `<button class="sw ${i.enabled ? "on" : ""}" data-id="${esc(i.id)}" aria-label="on/off"></button>`}
    </div>`;
}
async function loadAutostart() {
  const items = await invoke("autostart_list");
  const gone = items.filter((i) => i.missing), live = items.filter((i) => !i.missing);
  const on = live.filter((i) => i.enabled), off = live.filter((i) => !i.enabled);
  $("#t-auto").textContent = `${on.length} on`;
  $("#t-auto-s").textContent = `${off.length} turned off` + (gone.length ? ` · ${gone.length} leftover` : "");
  $("#auto-counts").innerHTML = `<span class="on">${on.length} on</span><span>${off.length} off</span>` + (gone.length ? `<span>${gone.length} leftover</span>` : "");
  $("#auto-list").innerHTML =
    (gone.length ? `<div class="group">Leftovers of removed apps</div>` + gone.map(startRow).join("") : "") +
    (on.length ? `<div class="group">Enabled</div>` + on.map(startRow).join("") : "") +
    (off.length ? `<div class="group">Disabled</div>` + off.map(startRow).join("") : "");
  $("#auto-list").querySelectorAll("[data-rm]").forEach((b) => (b.onclick = async () => {
    const ok = await confirmBox("Remove leftover entry?", "This app isn't installed anymore. Only the dead startup entry gets removed.", [[b.dataset.name, "not installed"]]);
    if (!ok) return;
    try { await invoke("autostart_remove", { id: b.dataset.rm }); toast("Removed"); }
    catch (e) { toast(String(e), true); }
    await loadAutostart();
  }));
  $("#auto-list").querySelectorAll(".sw").forEach((s) => (s.onclick = async () => {
    const want = !s.classList.contains("on");
    s.classList.add("busy");
    try {
      await invoke("autostart_set", { id: s.dataset.id, enabled: want });
      toast(want ? "Starts with Windows again" : "Won't start with Windows anymore");
    } catch (e) { toast(String(e), true); }
    await loadAutostart();
  }));
  // real app icons, loaded once in the background
  const missing = [...new Set(items.map((i) => i.icon).filter((s) => s && !(s in iconCache)))];
  if (missing.length) {
    missing.forEach((m) => (iconCache[m] = null));
    const got = await invoke("icons", { sources: missing });
    Object.assign(iconCache, got);
    document.querySelectorAll(".app-ic[data-icon]").forEach((el) => {
      const src = iconCache[el.dataset.icon];
      if (src && !el.querySelector("img")) el.innerHTML = `<img src="${src}" alt="">`;
    });
  }
}

/* ---------- themes ---------- */
const THEME_ICON = { startup: "i-monitor", saver: "i-moon", icons: "i-grid", cursor: "i-cursor", discord: "i-chat" };
function renderThemes(all) {
  const list = all.filter((t) => t.available);      // only what is really on this PC
  const on = list.filter((t) => t.on).length;
  $("#t-themes").textContent = `${on} of ${list.length}`;
  $("#t-themes-s").textContent = "turned on";
  $("#theme-counts").innerHTML = `<span class="on">${on} on</span><span>${list.length - on} off</span>`;
  $("#theme-grid").innerHTML = list.map((t) => `
    <div class="row theme ${t.on ? "is-on" : ""}">
      <svg class="ico"><use href="#${THEME_ICON[t.id] || "i-spark"}"/></svg>
      <div class="main">
        <div class="name">${esc(t.name)}</div>
        <div class="sub">${esc(t.desc)}</div>
      </div>
      <button class="sw ${t.on ? "on" : ""}" data-id="${t.id}" aria-label="on/off"></button>
    </div>`).join("") || `<div class="empty">${icon("i-spark")}No desktop themes on this PC.</div>`;
  $("#theme-grid").querySelectorAll(".sw").forEach((s) => (s.onclick = async () => {
    const want = !s.classList.contains("on");
    s.classList.add("busy");
    $("#theme-grid").querySelectorAll(".sw").forEach((x) => (x.disabled = true));
    try {
      renderThemes(await invoke("theme_set", { id: s.dataset.id, on: want }));
      toast(want ? "Turned on" : "Turned off");
    } catch (e) {
      toast(String(e).slice(0, 160) || "That didn't work", true);
      renderThemes(await invoke("themes"));
    }
  }));
}
async function loadThemes() {
  const list = await invoke("themes");
  renderThemes(list);
  if (!list.some((t) => t.available)) try {
    if (!localStorage.getItem("themes-checked")) { localStorage.setItem("themes-checked", "1"); await hideTab("themes", true); }
  } catch {}
}

/* ---------- jarvis ---------- */
const J = "http://127.0.0.1:8765";
const jv = { status: null, state: "idle", rec: null, chunks: [], stream: null, ctx: null, analyser: null, since: -1, button: "x2", busy: false };

function chip(cls, text) {
  const c = $("#j-chip");
  c.className = "chip " + cls;
  c.querySelector("span").textContent = text;
}
function jLine(cls, text) {
  const log = $("#j-log");
  log.querySelector(".idle")?.remove();
  const d = document.createElement("div");
  d.className = "j-line " + cls;
  d.textContent = text;
  log.append(d);
  while (log.children.length > 6) log.firstElementChild.remove();
}
function jState(s) {
  jv.state = s;
  $("#j-orb").classList.toggle("think", s === "thinking");
  $("#j-orb").classList.toggle("rec", s === "listening");
  const ready = jv.status?.running;
  if (!jv.status) return;
  if (!jv.status.running && !jv.status.python) return chip("warn", "Needs Python");
  if (!jv.status.groq) return chip("warn", "Needs API keys");
  if (!ready) return chip("warn", "Offline");
  chip("on" + (s !== "idle" ? " busy" : ""), { idle: "Online", listening: "Listening", thinking: "Thinking", speaking: "Speaking" }[s]);
}
const MOUSE = { x1: "Mouse back", x2: "Mouse forward", middle: "Mouse wheel button" };
const talkText = (b) => MOUSE[b] || b.replaceAll("+", " + ");
const holdText = (b) => (MOUSE[b] ? `Hold your ${{ x1: "back", x2: "forward", middle: "middle" }[b]} mouse button and talk.` : `Hold ${talkText(b)} and talk.`);
function jHint() {
  const idle = $("#j-idle");
  if (!idle) return;
  idle.textContent = jv.status?.running
    ? holdText(jv.status.talk || jv.button)
    : jv.status?.groq && jv.status?.python ? "Jarvis is starting …" : "Click here to set up Jarvis.";
}

async function jarvisRefresh(autostart) {
  try {
    let s = await invoke("jarvis_status");
    if (!s.running && autostart && s.python && s.groq && !cs?.hidden.includes("jarvis")) {
      jv.status = s; jState("idle"); chip("warn busy", "Starting …");
      try { s = await invoke("jarvis_start"); } catch (e) { toast(String(e), true); }
    }
    const was = jv.status?.running;
    jv.status = s;
    if (s.running && !was) {
      fetch(J + "/jarvis/config").then((r) => r.json()).then((c) => { jv.button = c.button || "x2"; jHint(); }).catch(() => {});
      watchButton();
    }
    jState(jv.state); jHint();
  } catch (e) { console.error(e); }
}

/* level meter for the orb (mic while listening, voice while speaking) */
function audioCtx() { return (jv.ctx ||= new AudioContext()); }
function meter() {
  const data = new Uint8Array(256);
  const loop = () => {
    let lvl = 0;
    if (jv.analyser && (jv.state === "listening" || jv.state === "speaking")) {
      jv.analyser.getByteTimeDomainData(data);
      let sum = 0;
      for (const v of data) sum += ((v - 128) / 128) ** 2;
      lvl = Math.min(1, Math.sqrt(sum / data.length) * 4);
    }
    const o = $("#j-orb"), cur = +o.style.getPropertyValue("--lvl") || 0;
    o.style.setProperty("--lvl", (cur + (lvl - cur) * 0.3).toFixed(3));
    requestAnimationFrame(loop);
  };
  loop();
}

async function startTalk() {
  if (jv.rec || jv.busy || !jv.status?.running) return;
  if (jv.state === "speaking") jv.player?.stop();
  try {
    jv.stream ||= await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true } });
  } catch (e) { toast("No microphone access: " + e.message, true); return; }
  const ctx = audioCtx(); ctx.resume();
  jv.analyser = ctx.createAnalyser(); jv.analyser.fftSize = 512;
  ctx.createMediaStreamSource(jv.stream).connect(jv.analyser);
  jv.chunks = [];
  jv.rec = new MediaRecorder(jv.stream, { mimeType: "audio/webm;codecs=opus" });
  jv.rec.ondataavailable = (e) => e.data.size && jv.chunks.push(e.data);
  jv.rec.onstop = () => handleTalk(new Blob(jv.chunks, { type: "audio/webm" }), performance.now() - jv.t0);
  jv.t0 = performance.now();
  jv.rec.start(250);
  jState("listening");
}
function stopTalk() {
  if (!jv.rec) return;
  const r = jv.rec; jv.rec = null;
  if (r.state !== "inactive") r.stop();
}

async function post(path, body, raw) {
  const r = await fetch(J + path, { method: "POST", headers: { "Content-Type": raw ? "audio/webm" : "application/json" }, body: raw ? body : JSON.stringify(body) });
  return r;
}
async function handleTalk(blob, ms) {
  if (ms < 350 || blob.size < 2000) { jState("idle"); return; }   // just a tap
  jv.busy = true; jState("thinking");
  try {
    const t = await (await post("/jarvis/transcribe", blob, true)).json();
    if (t.error) throw new Error(t.error);
    if (!t.text) { jState("idle"); jv.busy = false; return; }
    jLine("you", t.text);
    const a = await (await post("/jarvis/ask", { text: t.text })).json();
    if (a.error && !a.text) throw new Error(a.error);
    jLine("bot", a.text);
    await say(a.text);
  } catch (e) {
    jLine("err", String(e.message || e));
  }
  jv.busy = false; jState("idle");
}
async function say(text) {
  if (!jv.status?.eleven) return;          // no voice key: text only
  const r = await post("/jarvis/speak", { text });
  if (!r.ok) { const e = await r.json().catch(() => ({})); jLine("err", e.error || "Voice failed"); return; }
  const ctx = audioCtx(); await ctx.resume();
  const buf = await ctx.decodeAudioData(await r.arrayBuffer());
  const src = ctx.createBufferSource(); src.buffer = buf;
  jv.analyser = ctx.createAnalyser(); jv.analyser.fftSize = 512;
  src.connect(jv.analyser); jv.analyser.connect(ctx.destination);
  jv.player = src;
  jState("speaking");
  await new Promise((done) => { src.onended = done; src.start(); });
  jv.player = null;
}

/* the mouse thumb button, via the Jarvis server's global hook */
let watching = false;
async function watchButton() {
  if (watching) return; watching = true;
  while (jv.status?.running) {
    try {
      const r = await (await fetch(`${J}/jarvis/trigger?since=${jv.since}`)).json();
      if (jv.since >= 0 && r.events !== jv.since) r.down ? startTalk() : stopTalk();
      jv.since = r.events;
    } catch { await new Promise((d) => setTimeout(d, 3000)); jarvisRefresh(false); }
  }
  watching = false;
}

// holding the orb also talks (the click still belongs to the key dialog everywhere else)
const orb = $("#j-orb");
orb.addEventListener("pointerdown", (e) => { if (!jv.status?.running) return; e.stopPropagation(); orb.setPointerCapture(e.pointerId); startTalk(); });
orb.addEventListener("pointerup", (e) => { if (jv.rec) { e.stopPropagation(); stopTalk(); } });
orb.addEventListener("pointercancel", stopTalk);
orb.addEventListener("click", (e) => { if (jv.status?.running) e.stopPropagation(); });

// "Ctrl+Alt+F8"; letters by what is printed on the key, so Y and Z are right on a German keyboard
function keyName(e) {
  const c = e.code.replace(/^(Key|Digit)/, "");
  const name = e.code.startsWith("Key") && e.keyCode >= 65 && e.keyCode <= 90 ? String.fromCharCode(e.keyCode) : c;
  return (e.ctrlKey ? "Ctrl+" : "") + (e.altKey ? "Alt+" : "") + (e.shiftKey ? "Shift+" : "") + name;
}
const ONLY_MOD = ["Control", "Alt", "Shift", "Meta", "AltGraph"];

/* talk key inside the dialog */
let talk = "x2", talkWait = false;
function setTalk(v) { talk = v; talkWait = false; $("#k-talk").classList.remove("wait"); $("#k-talk").textContent = talkText(v); }
$("#k-talk").onclick = () => { if (talkWait) return; talkWait = true; $("#k-talk").classList.add("wait"); $("#k-talk").textContent = "Press a key …"; $("#k-talk").focus(); };
$("#k-talk").onblur = () => { if (talkWait) setTalk(talk); };
document.addEventListener("keydown", (e) => {
  if (!talkWait) return;
  e.preventDefault(); e.stopPropagation();
  if (ONLY_MOD.includes(e.key)) return;
  if (e.key === "Escape") return setTalk(talk);
  const k = keyName(e);
  if (cs && (k === cs.hotkey || k === cs.rec_hotkey)) { toast("That key already belongs to Clips.", true); return setTalk(talk); }
  if (!/^((Ctrl|Alt|Shift)\+)*(F\d{1,2}|Insert|Delete|Home|End|PageUp|PageDown|Pause)$/.test(k) && !/^(?=.*(Ctrl|Alt)\+)((Ctrl|Alt|Shift)\+)+[A-Z0-9]$/.test(k)) { toast("That key can't be used. Try F9 or Alt + a letter.", true); return setTalk(talk); }
  setTalk(k);
}, true);
for (const ev of ["mousedown", "mouseup", "auxclick"]) document.addEventListener(ev, (e) => {
  const b = { 1: "middle", 3: "x1", 4: "x2" }[e.button];
  if (!b) return;
  if (b !== "middle" || talkWait) e.preventDefault();      // the side buttons must not leave the page
  if (talkWait && ev === "mousedown") { e.stopPropagation(); setTalk(b); }
}, true);

/* setup dialog: opens when you click the panel */
let lang = "en";
function setLang(v) { lang = v; document.querySelectorAll("#k-lang button").forEach((b) => b.classList.toggle("on", b.dataset.v === v)); }
document.querySelectorAll("#k-lang button").forEach((b) => (b.onclick = () => setLang(b.dataset.v)));
document.querySelectorAll("#keys .lnk").forEach((b) => (b.onclick = () => invoke("open_url", { url: b.dataset.url }).catch((e) => toast(String(e), true))));
function closeKeys() { $("#keys").classList.remove("open"); $("#k-groq").value = $("#k-eleven").value = ""; }
$("#jarvis").onclick = () => {
  const s = jv.status || {};
  $("#k-groq").placeholder = s.groq ? `Saved · ends in ${s.groq}` : "gsk_…";
  $("#k-eleven").placeholder = s.eleven ? `Saved · ends in ${s.eleven}` : "sk_…";
  $("#k-py").hidden = !!s.python;
  setLang(s.lang === "de" ? "de" : "en");
  setTalk(s.talk || "x2");
  $("#keys").classList.add("open");
  setTimeout(() => $("#k-groq").focus(), 50);
};
$("#k-no").onclick = closeKeys;
$("#keys").onclick = (e) => { if (e.target.id === "keys") closeKeys(); };
$("#keys").addEventListener("keydown", (e) => { if (e.key === "Escape") closeKeys(); if (e.key === "Enter" && e.target.id !== "k-talk") $("#k-yes").click(); });
$("#k-yes").onclick = async () => {
  const groq = $("#k-groq").value.trim(), eleven = $("#k-eleven").value.trim();
  if (!groq && !eleven && lang === (jv.status?.lang || "en") && talk === (jv.status?.talk || "x2")) { closeKeys(); return; }
  const btn = $("#k-yes");
  btn.disabled = true; btn.innerHTML = `<span class="spin"></span>Saving …`;
  try {
    jv.status = await invoke("jarvis_save", { groq, eleven, lang, talk });
    closeKeys();
    toast("Saved. Jarvis restarted.");
    jv.since = -1;
    fetch(J + "/jarvis/config").then((r) => r.json()).then((c) => { jv.button = c.button || "x2"; jHint(); }).catch(() => {});
    watchButton();
  } catch (e) { toast(String(e), true); }
  btn.disabled = false; btn.textContent = "Save";
  jState("idle"); jHint();
};

/* ---------- clips ---------- */
let cs = null, clips = [], playing = null, keyWait = null, filter = "";
const clock = (s) => { s = Math.round(s); const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60; return (h ? h + ":" + String(m).padStart(2, "0") : m) + ":" + String(s % 60).padStart(2, "0"); };
const lenText = (s) => (s < 60 ? `${s} s` : s % 60 ? clock(s) : `${s / 60} min`);
// "45", "90s", "2 min", "2m", "1:30", "1m 30s", "1,5 min"
function parseLen(t) {
  t = t.trim().toLowerCase().replace(",", ".");
  let m;
  if ((m = t.match(/^(\d+):(\d{1,2})$/))) return +m[1] * 60 + +m[2];
  if ((m = t.match(/^(\d+(?:\.\d+)?)\s*(?:s|sec|secs|seconds?|sek|sekunden?)?$/))) return Math.round(+m[1]);
  if ((m = t.match(/^(\d+(?:\.\d+)?)\s*(?:m|min|mins|minutes?|minuten?)\s*(?:(\d+)\s*(?:s|sec|sek)?)?$/))) return Math.round(+m[1] * 60 + +(m[2] || 0));
  return null;
}
function showClipState(s) {
  cs = s;
  const rec = s.recording != null, live = s.on && s.active && !s.error;
  $("#c-on").classList.toggle("on", s.on);
  $("#c-title").textContent = s.on ? (s.active ? "Clipping is on" : "Starting …") : "Clipping is off";
  const bad = s.error || s.hotkey_error;
  if (!fetching) $("#c-sub").textContent = bad || (s.on ? "Runs in the background, also with the window closed." : "Switch it on to keep the last seconds of your screen.");
  $("#c-sub").classList.toggle("bad", !!bad);
  // clip
  $("#m-clip").classList.toggle("dimmed", !s.on);
  $("#c-lentext").textContent = lenText(s.secs);
  $("#c-clipsub").textContent = s.on ? "Saves what just happened" : "Switch clipping on first";
  $("#c-save").disabled = !live || s.saving;
  $("#c-save span").textContent = s.saving ? "Saving …" : "Save clip";
  // recording
  $("#m-rec").classList.toggle("is-rec", rec);
  $("#c-rectime").textContent = rec ? clock(s.recording) : "";
  $("#c-recsub").textContent = rec ? "Recording now" : "Records until you stop it";
  $("#c-rec").classList.toggle("rec", rec);
  $("#c-rec span").textContent = rec ? "Stop recording" : "Start recording";
  $("#nav-rec").className = "nav-rec" + (rec ? " rec" : live ? " on" : "");
  if (document.activeElement !== $("#c-len")) $("#c-len").value = lenText(s.secs);
  document.querySelectorAll("#c-quick button").forEach((b) => b.classList.toggle("on", +b.dataset.s === s.secs));
  document.querySelectorAll(".keycap[data-key]").forEach((k) => { if (k !== keyWait) k.textContent = s[k.dataset.key] || "None"; });
  // look, name, hidden tabs
  if (s.look !== look.name || s.fx !== look.fx) setLook(s.look, s.fx);
  $("#brand").textContent = s.name || s.user || "PC";
  if (document.activeElement !== $("#s-name")) { $("#s-name").value = s.name; $("#s-name").placeholder = s.user; }
  const hid = s.hidden.join();
  if (hid !== showClipState.hid) { showClipState.hid = hid; applyHidden(s.hidden); }
  document.querySelectorAll("[data-show]").forEach((b) => b.classList.toggle("on", !s.hidden.includes(b.dataset.show)));
  $("#t-clips").textContent = s.on ? "On" : "Off";
  $("#t-clips-s").textContent = s.on ? `Last ${lenText(s.secs)}` + (s.hotkey ? ` · ${s.hotkey}` : "") : "Screen clips";
  // settings page
  document.querySelectorAll("[data-set]").forEach((b) => b.classList.toggle("on", !!s[b.dataset.set]));
  document.querySelectorAll("[data-seg]").forEach((g) => g.querySelectorAll("button").forEach((b) => b.classList.toggle("on", +b.dataset.v === s[g.dataset.seg])));
  for (const el of document.querySelectorAll('[data-set="mic"], [data-seg="fps"] button, [data-seg="quality"] button')) el.disabled = rec;
  document.querySelectorAll("#s-lang button").forEach((b) => b.classList.toggle("on", b.dataset.v === s.lang));
  $("#s-folder").textContent = s.folder; $("#s-folder").title = s.folder;
}
async function clipState() { try { showClipState(await invoke("clips_state")); } catch (e) { console.error(e); } }
async function clipSet(patch) {
  try { showClipState(await invoke("clips_config", { patch })); } catch (e) { toast(String(e), true); clipState(); }
}
$("#c-on").onclick = async () => { if (cs && (cs.on || (await needRecorder()))) clipSet({ on: !cs.on }); };
// the recorder (ffmpeg) is loaded once, the first time it is needed
let fetching = false;
async function needRecorder() {
  if (cs.recorder) return true;
  if (fetching) return false;
  const ok = await confirmBox("Download the recorder?", "Clips need ffmpeg, a free screen recorder. It is loaded once from GitHub.", [["ffmpeg 9.1", "37 MB"]]);
  if (!ok) return false;
  fetching = true;
  try { await invoke("recorder_install"); await clipState(); toast("Recorder installed"); }
  catch (e) { toast(String(e), true); }
  fetching = false;
  clipState();
  return cs.recorder;
}
T.event.listen("recorder-progress", ({ payload }) => { $("#c-sub").textContent = `Downloading recorder … ${payload}%`; });
document.querySelectorAll("#c-quick button").forEach((b) => (b.onclick = () => cs && clipSet({ secs: +b.dataset.s })));
$("#c-len").onfocus = (e) => e.target.select();
$("#c-len").onkeydown = (e) => { if (e.key === "Enter") e.target.blur(); if (e.key === "Escape") { e.target.value = lenText(cs.secs); e.target.blur(); } };
$("#c-len").onblur = () => {
  if (!cs) return;
  let v = parseLen($("#c-len").value);
  if (v == null) { toast("Type something like 45 s, 2 min or 1:30", true); $("#c-len").value = lenText(cs.secs); return; }
  if (v > 600 || v < 5) toast("Between 5 seconds and 10 minutes", true);
  v = Math.min(600, Math.max(5, v));
  $("#c-len").value = lenText(v);
  if (v !== cs.secs) clipSet({ secs: v });
};
$("#c-save").onclick = () => invoke("clips_save");
$("#c-rec").onclick = async () => {
  if (!(await needRecorder())) return;
  $("#c-rec").disabled = true;
  try { showClipState(await invoke("clips_record")); } catch (e) { toast(String(e), true); clipState(); }
  $("#c-rec").disabled = false;
};
document.querySelectorAll(".keycap[data-key]").forEach((k) => {
  k.onclick = () => { keyWait = k; k.textContent = "Press a key …"; k.classList.add("wait"); k.focus(); };
  k.onblur = () => { if (keyWait === k) keyWait = null; k.classList.remove("wait"); if (cs) k.textContent = cs[k.dataset.key] || "None"; };
});
document.addEventListener("keydown", (e) => {
  if (!keyWait) return;
  e.preventDefault(); e.stopPropagation();
  if (ONLY_MOD.includes(e.key)) return;
  const k = keyWait;
  if (e.key !== "Escape") {
    const key = e.key === "Backspace" ? "" : keyName(e);
    if (key && key === jv.status?.talk) toast("That key already belongs to Jarvis.", true);
    else clipSet({ [k.dataset.key]: key });
  }
  k.blur();
}, true);

/* ---------- settings ---------- */
async function loadSettings() {
  clipState();
  checkUpdates(false);
  try { $("#s-auto").classList.toggle("on", await invoke("app_autostart", { on: null })); } catch (e) { console.error(e); }
}
document.querySelectorAll("[data-set]").forEach((b) => (b.onclick = () => cs && clipSet({ [b.dataset.set]: !cs[b.dataset.set] })));
document.querySelectorAll("[data-seg] button").forEach((b) => (b.onclick = () => clipSet({ [b.parentElement.dataset.seg]: +b.dataset.v })));
$("#s-auto").onclick = async () => {
  const b = $("#s-auto"), want = !b.classList.contains("on");
  b.classList.add("busy");
  try { b.classList.toggle("on", await invoke("app_autostart", { on: want })); toast(want ? "Starts with Windows now" : "Won't start with Windows anymore"); }
  catch (e) { toast(String(e), true); }
  b.classList.remove("busy");
};
$("#s-folder-btn").onclick = async () => { try { showClipState(await invoke("clips_folder")); } catch (e) { toast(String(e), true); } };
document.querySelectorAll("#s-lang button").forEach((b) => (b.onclick = async () => {
  if (b.dataset.v === LANG && cs?.lang === LANG) return;
  await clipSet({ lang: b.dataset.v });
  useLang(b.dataset.v);
}));
document.querySelectorAll("[data-show]").forEach((b) => (b.onclick = () => cs && hideTab(b.dataset.show, b.classList.contains("on"))));
document.querySelectorAll("#look-grid .look").forEach((b) => (b.onclick = () => clipSet({ look: b.dataset.look })));
$("#s-name").onkeydown = (e) => { if (e.key === "Enter") e.target.blur(); };
$("#s-name").onblur = () => { const v = $("#s-name").value.trim(); if (cs && v !== cs.name) clipSet({ name: v }); };

/* ---------- updates ---------- */
const upd = { data: null, at: 0, busy: false };
const ver = (v) => "V" + v.replace(/^(\d+\.\d+)\.0$/, "$1");
function notes(text) {
  let out = "", open = false;
  for (const raw of text.split(/\r?\n/)) {
    const l = raw.trim().replace(/\*\*/g, "");
    const item = l.match(/^[-*] (.+)/);
    if (item) { out += (open ? "" : "<ul>") + `<li>${esc(item[1])}</li>`; open = true; continue; }
    if (open) { out += "</ul>"; open = false; }
    if (l && !l.startsWith("#")) out += `<p>${esc(l)}</p>`;
  }
  return out + (open ? "</ul>" : "");
}
function renderUpdates(err) {
  const d = upd.data, now = cs?.version || "";
  $("#u-sub").textContent = err ? err : !d ? `${ver(now)} installed` : d.newer ? `${ver(d.releases[0].version)} is available` : `${ver(now)} is up to date`;
  $("#u-sub").classList.toggle("bad", !!err);
  $("#u-go").hidden = !d?.newer;
  $("#nav-upd").className = "nav-rec" + (d?.newer ? " on" : "");
  if (d) $("#u-list").innerHTML = d.releases.length ? d.releases.map((r) => `
    <div class="u-rel ${r.version === d.current ? "cur" : ""}">
      <div class="u-top"><b>${ver(r.version)}</b>${r.version === d.current ? `<span class="badge sys">Installed</span>` : r === d.releases[0] && d.newer ? `<span class="badge safe">New</span>` : ""}<span class="u-date">${new Date(r.date).toLocaleDateString(LOC, { day: "numeric", month: "long", year: "numeric" })} · ${gb(r.size)}</span></div>
      <div class="u-notes">${notes(r.notes)}</div>
    </div>`).join("") : `<div class="empty">Nothing published yet.</div>`;
}
async function checkUpdates(force) {
  if (upd.busy || (!force && Date.now() - upd.at < 20 * 60e3)) return;
  upd.busy = true;
  $("#u-check span").textContent = "Checking …";
  try { upd.data = await invoke("updates_check"); upd.at = Date.now(); renderUpdates(); if (force) toast(upd.data.newer ? "Update found" : "Everything is up to date"); }
  catch (e) { renderUpdates(String(e)); }
  $("#u-check span").textContent = "Check";
  upd.busy = false;
}
$("#u-check").onclick = () => checkUpdates(true);
$("#u-go").onclick = async () => {
  upd.busy = true;
  $("#u-go").disabled = $("#u-check").disabled = true;
  $("#u-bar").hidden = false;
  try { await invoke("update_install"); $("#u-go span").textContent = "Restarting …"; }
  catch (e) {
    toast(String(e), true);
    $("#u-go").disabled = $("#u-check").disabled = false; $("#u-go span").textContent = "Install"; $("#u-bar").hidden = true; upd.busy = false;
  }
};
T.event.listen("update-progress", ({ payload }) => { $("#u-bar i").style.width = payload + "%"; $("#u-go span").textContent = payload + "%"; });
setInterval(() => checkUpdates(false), 3600e3);

// the whole page is redrawn in the new language and comes back where you were
function useLang(v) {
  invoke("app_lang").catch(() => {});
  try { localStorage.setItem("lang", v); sessionStorage.setItem("page", current); } catch {}
  location.reload();
}

function clipDate(ms) {
  const d = new Date(ms), t = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  const days = Math.round((new Date().setHours(0, 0, 0, 0) - new Date(ms).setHours(0, 0, 0, 0)) / 864e5);
  return (days === 0 ? (LANG === "de" ? "Heute" : "Today") : days === 1 ? (LANG === "de" ? "Gestern" : "Yesterday") : d.toLocaleDateString(LOC, { month: "short", day: "numeric", year: "numeric" })) + ", " + t;
}
async function loadClips() {
  clips = (await invoke("clips_list")).filter((c) => !filter || c.name.startsWith("Recording") === (filter === "Recording"));
  const size = clips.reduce((s, c) => s + c.size, 0);
  $("#c-count").textContent = clips.length ? `${clips.length} clip${clips.length > 1 ? "s" : ""} · ${gb(size)}` : "";
  $("#c-grid").innerHTML = clips.length ? clips.map((c, i) => `
    <button class="clip" data-i="${i}">
      <span class="th">${c.thumb ? `<img loading="lazy" alt="" src="${esc(T.core.convertFileSrc(c.thumb))}">` : ""}<i>${clock(c.secs)}</i><svg class="play"><use href="#i-film"/></svg></span>
      <span class="cn">${clipDate(c.time)}</span><span class="cs">${c.name.startsWith("Recording") ? "Recording" : "Clip"} · ${gb(c.size)}</span>
    </button>`).join("") : `<div class="empty">${icon("i-film")}${filter ? "Nothing here yet." : "No clips yet."}</div>`;
  document.querySelectorAll(".clip").forEach((b) => (b.onclick = () => openPlayer(clips[+b.dataset.i])));
}
document.querySelectorAll("#c-filter button").forEach((b) => (b.onclick = () => {
  filter = b.dataset.f;
  document.querySelectorAll("#c-filter button").forEach((x) => x.classList.toggle("on", x === b));
  loadClips().catch(console.error);
}));
function openPlayer(c) {
  playing = c;
  $("#p-name").textContent = clipDate(c.time);
  $("#p-sub").textContent = `${clock(c.secs)} · ${gb(c.size)}`;
  $("#p-video").src = T.core.convertFileSrc(c.path);
  $("#player").classList.add("open");
  $("#p-video").play().catch(() => {});
}
function closePlayer() {
  if (!playing) return;
  playing = null;
  const v = $("#p-video"); v.pause(); v.removeAttribute("src"); v.load();   // lets go of the file
  $("#player").classList.remove("open");
}
$("#p-close").onclick = closePlayer;
$("#player").onclick = (e) => { if (e.target.id === "player") closePlayer(); };
document.addEventListener("keydown", (e) => { if (e.key === "Escape") closePlayer(); });
$("#p-copy").onclick = async () => { try { await invoke("clips_copy", { name: playing.name }); toast("Copied. Paste it into Discord with Ctrl + V."); } catch (e) { toast(String(e), true); } };
$("#p-show").onclick = () => invoke("clips_show", { name: playing.name }).catch((e) => toast(String(e), true));
$("#p-del").onclick = async () => {
  const c = playing;
  closePlayer();
  if (!(await confirmBox("Delete this clip?", "It goes to the Recycle Bin.", [[clipDate(c.time), gb(c.size)]]))) return;
  try { await invoke("clips_delete", { name: c.name }); toast("Moved to the Recycle Bin"); } catch (e) { toast(String(e), true); }
  loadClips().catch(console.error);
};
$("#c-folder").onclick = () => invoke("clips_show", { name: "" }).catch((e) => toast(String(e), true));
T.event.listen("clips", ({ payload: [kind, text] }) => {
  clipState();
  if (kind === "recording") toast("Recording started");
  if (kind === "saved") { toast("Saved"); if (current === "clips") loadClips().catch(console.error); }
  if (kind === "error") toast(text, true);
});
setInterval(() => { if (!hidden && (current === "clips" || cs?.recording != null)) clipState(); }, 1000);

/* ---------- start ---------- */
(async () => {
  $("#subtitle").textContent = greeting();
  moveIndicator();
  try { await loadDisk(); } catch (e) { console.error(e); }
  await tick();
  if (await invoke("first_show")) win.show();
  else hidden = true;
  setInterval(tick, 1500);
  await clipState();
  if (cs && cs.lang !== LANG) return useLang(cs.lang);    // the saved setting wins
  try { const p = sessionStorage.getItem("page"); sessionStorage.removeItem("page"); if (PAGES[p]) go(p); } catch {}
  loadThemes().catch(console.error);
  loadCleanup().catch(console.error);
  loadAutostart().catch(console.error);
  meter();
  renderUpdates();
  checkUpdates(false);
  jarvisRefresh(true);
  setInterval(() => jarvisRefresh(false), 10000);
})();
