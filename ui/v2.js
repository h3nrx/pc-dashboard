// PC Dashboard V2: sidebar, Gaming, Sound, Notes, Downloads
Object.assign(PAGES, {
  gaming: ["Gaming", () => "Crosshair and ping"],
  sound: ["Sound", () => "What's playing and the volume of every app"],
  notes: ["Notes", () => "Sticky notes that stay on top of everything"],
});
const store = {
  get(k, d) { try { const v = localStorage.getItem(k); return v == null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
};
const pct = (el) => el.style.setProperty("--p", ((el.value - el.min) / (el.max - el.min)) * 100 + "%");
// a keycap that saves somewhere else than the clip settings
function hotkeyCap(el, get, set) {
  el.onclick = () => { keyWait = el; el.textContent = "Press a key …"; el.classList.add("wait"); el.focus(); };
  el.onblur = () => { if (keyWait === el) keyWait = null; el.classList.remove("wait"); el.textContent = get() || "None"; };
  el._onkey = set;
  el.textContent = get() || "None";
}

/* ---------- sidebar: groups, can shrink to icons ---------- */
function setMini(on) {
  document.documentElement.classList.toggle("mini", on);
  $("#side-mini").title = on ? "Expand sidebar" : "Collapse sidebar";
  store.set("mini", on);
  setTimeout(moveIndicator, 320);
}
$("#side-mini").onclick = () => setMini(!document.documentElement.classList.contains("mini"));
if (store.get("mini", false)) setMini(true);
// a group label disappears when every tab in it is hidden
const _applyHidden = applyHidden;
applyHidden = function (list) {
  _applyHidden(list);
  document.querySelectorAll(".nav-group").forEach((g) => {
    let n = g.nextElementSibling, any = false;
    while (n && !n.classList.contains("nav-group")) { if (n.classList.contains("nav") && !n.hidden && !n.classList.contains("nav-last")) any = true; n = n.nextElementSibling; }
    g.hidden = !any;
  });
  moveIndicator();
};

const _go = go;
go = function (page) {
  _go(page);
  if (page === "gaming") { crossLoad(); pingTick(); }
  if (page === "sound") { nowTick(); volTick(true); }
  if (page === "notes") notesLoad();
  if (page === "storage") dlLoad();
  if (page === "settings") { crossLoad(); notesLoad(); jarvisKey(); }
};
document.querySelectorAll(".nav").forEach((b) => (b.onclick = () => go(b.dataset.page)));
const visible = (p) => !hidden && !document.hidden && current === p;

/* ================= GAMING ================= */
let xc = null;
const XCOLORS = ["#00ff9c", "#ffffff", "#ff3b5c", "#ffe14d", "#33d1ff", "#ff6bff"];
$("#x-colors").innerHTML = XCOLORS.map((c) => `<button data-c="${c}" style="background:${c}" title="${c}"></button>`).join("");
function crossShow(c) {
  xc = c;
  $("#x-on").classList.toggle("on", c.on);
  $("#nav-cross").className = "nav-rec" + (c.on ? " on" : "");
  $("#x-svg").setAttribute("viewBox", "0 0 220 220");
  $("#x-svg").innerHTML = crossSVG(c, 220);
  document.querySelectorAll("#x-shape button").forEach((b) => b.classList.toggle("on", b.dataset.v === c.shape));
  document.querySelectorAll("#x-colors button").forEach((b) => b.classList.toggle("on", b.dataset.c.toLowerCase() === c.color.toLowerCase()));
  $("#x-color").value = c.color;
  for (const [id, k, f] of [["x-size", "size", (v) => v + " px"], ["x-thick", "thick", (v) => v + " px"], ["x-gap", "gap", (v) => v + " px"], ["x-op", "opacity", (v) => Math.round(v * 100) + "%"]]) {
    const el = $("#" + id);
    if (document.activeElement !== el) el.value = c[k];
    pct(el);
    $("#" + id + "-v").textContent = f(+el.value);
  }
  $("#x-dot").classList.toggle("on", c.dot);
  $("#x-dot").disabled = c.shape === "dot";
  $("#x-outline").classList.toggle("on", c.outline);
  $("#x-gap").disabled = ["dot", "circle"].includes(c.shape);
  if (keyWait !== $("#x-key")) $("#x-key").textContent = c.hotkey || "None";
  if (keyWait !== $("#s-key-cross")) $("#s-key-cross").textContent = c.hotkey || "None";
}
async function crossLoad() { try { crossShow(await invoke("cross_get")); } catch (e) { console.error(e); } }
let xTimer = null;
async function crossSet(patch, now = true) {
  if (!xc) return;
  crossShow({ ...xc, ...patch });          // the preview follows at once
  clearTimeout(xTimer);
  const send = async () => { try { crossShow(await invoke("cross_set", { patch })); } catch (e) { toast(String(e), true); crossLoad(); } };
  if (now) send(); else xTimer = setTimeout(send, 120);
}
$("#x-on").onclick = () => xc && crossSet({ on: !xc.on });
$("#x-dot").onclick = () => xc && crossSet({ dot: !xc.dot });
$("#x-outline").onclick = () => xc && crossSet({ outline: !xc.outline });
document.querySelectorAll("#x-shape button").forEach((b) => (b.onclick = () => crossSet({ shape: b.dataset.v })));
document.querySelectorAll("#x-colors button").forEach((b) => (b.onclick = () => crossSet({ color: b.dataset.c })));
$("#x-color").oninput = (e) => crossSet({ color: e.target.value }, false);
for (const [id, k] of [["x-size", "size"], ["x-thick", "thick"], ["x-gap", "gap"], ["x-op", "opacity"]]) $("#" + id).oninput = (e) => crossSet({ [k]: +e.target.value }, false);
hotkeyCap($("#x-key"), () => xc?.hotkey, (key) => crossSet({ hotkey: key }));
hotkeyCap($("#s-key-cross"), () => xc?.hotkey, (key) => crossSet({ hotkey: key }));
T.event.listen("cross-state", ({ payload }) => { crossShow(payload); toast(payload.on ? "Crosshair on" : "Crosshair off"); });

/* ping */
const PING_DEFAULT = [{ host: "1.1.1.1", name: "Cloudflare" }, { host: "8.8.8.8", name: "Google" }, { host: "discord.com", name: "Discord" }];
let pings = store.get("pings", PING_DEFAULT);
const phist = {};
function pingRows() {
  $("#p-list").innerHTML = pings.length ? pings.map((p) => `
    <div class="row ping" data-h="${esc(p.host)}" data-named="${p.name ? 1 : ""}">
      <span class="dot"></span>
      <div class="main"><div class="name">${esc(p.name || p.host)}</div><div class="sub">${p.name ? esc(p.host) : ""}<span class="pst"></span></div></div>
      <div class="pbar"><i></i></div>
      <b class="ms">–</b>
      <button class="icon-btn rm" title="Remove">${icon("i-close")}</button>
    </div>`).join("") : `<div class="empty">Add a server to see its ping.</div>`;
  $("#p-list").querySelectorAll(".rm").forEach((b) => (b.onclick = () => {
    pings = pings.filter((p) => p.host !== b.closest(".ping").dataset.h); store.set("pings", pings); pingRows();
  }));
}
pingRows();
$("#p-add").onsubmit = (e) => {
  e.preventDefault();
  const h = $("#p-host").value.trim().toLowerCase().replace(/^https?:\/\//, "").replace(/[\/:].*$/, "");
  if (!/^[a-z0-9.-]{1,253}$/.test(h)) return toast("That doesn't look like a server address", true);
  if (pings.some((p) => p.host === h)) return toast("Already in the list", true);
  if (pings.length >= 12) return toast("12 servers at most", true);
  pings.push({ host: h, name: "" }); store.set("pings", pings); $("#p-host").value = ""; pingRows(); pingTick();
};
let pingBusy = false;
async function pingTick() {
  if (!visible("gaming") || pingBusy || !pings.length) return;
  pingBusy = true;
  try {
    const res = await invoke("ping_hosts", { hosts: pings.map((p) => p.host) });
    for (const r of res) {
      const row = document.querySelector(`.ping[data-h="${CSS.escape(r.host)}"]`);
      if (!row) continue;
      const h = (phist[r.host] ||= []);
      h.push(r.ms); if (h.length > 15) h.shift();
      const ok = h.filter((x) => x != null);
      const cls = r.ms == null ? "bad" : r.ms < 50 ? "good" : r.ms < 110 ? "mid" : "bad";
      row.querySelector(".dot").className = "dot " + cls;
      row.querySelector(".ms").className = "ms " + cls;
      row.querySelector(".ms").textContent = r.ms == null ? "no answer" : r.ms + " ms";
      const bar = row.querySelector(".pbar i");
      bar.style.width = r.ms == null ? "100%" : Math.min(100, (r.ms / 200) * 100) + "%";
      bar.style.background = `var(--${cls === "good" ? "ok" : cls === "mid" ? "warn" : "bad-c"})`;
      if (ok.length > 2) {
        const avg = ok.reduce((a, b) => a + b, 0) / ok.length;
        const jit = ok.slice(1).reduce((a, b, i) => a + Math.abs(b - ok[i]), 0) / (ok.length - 1);
        const lost = h.length - ok.length;
        row.querySelector(".pst").textContent = `${row.dataset.named ? " · " : ""}avg ${Math.round(avg)} ms · jitter ${Math.round(jit)} ms` + (lost ? ` · ${lost} lost` : "");
      }
    }
  } catch (e) { console.error(e); }
  pingBusy = false;
}
setInterval(pingTick, 2000);

/* ================= SOUND ================= */
let np = null, npAt = 0;
const mmss = (s) => { s = Math.max(0, Math.floor(s || 0)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };
function nowShow(n) {
  np = n; npAt = performance.now();
  $("#np").classList.toggle("none", !n);
  $("#np-app").textContent = n ? n.app : "Now playing";
  $("#np-title").textContent = n ? n.title : tr("Nothing is playing");
  $("#np-artist").textContent = n ? [n.artist, n.album].filter(Boolean).join(" · ") : tr("Start a song in Spotify or anywhere else.");
  const art = $("#np-art");
  art.classList.toggle("pic", !!n?.art);
  art.style.backgroundImage = n?.art ? `url("${n.art}")` : "";
  $("#np-toggle").innerHTML = icon(n?.playing ? "i-pause" : "i-play");
  $("#np-prev").disabled = !n?.can_prev; $("#np-next").disabled = !n?.can_next; $("#np-toggle").disabled = !n;
  nowFrame();
}
function nowFrame() {
  if (!np) return;
  const pos = Math.min(np.len || Infinity, np.pos + (np.playing ? (performance.now() - npAt) / 1000 : 0));
  $("#np-fill").style.width = np.len ? (pos / np.len) * 100 + "%" : "0";
  $("#np-pos").textContent = np.len ? mmss(pos) : "";
  $("#np-len").textContent = np.len ? mmss(np.len) : "";
}
async function nowTick() { if (!visible("sound")) return; try { nowShow(await invoke("media_now")); } catch (e) { console.error(e); } }
setInterval(nowTick, 1500);
setInterval(() => visible("sound") && nowFrame(), 250);
const media = async (cmd, at) => { try { await invoke("media_cmd", { cmd, at: at ?? null }); setTimeout(nowTick, 250); } catch (e) { toast(String(e), true); } };
$("#np-toggle").onclick = () => { if (np) { np.pos += np.playing ? (performance.now() - npAt) / 1000 : 0; np.playing = !np.playing; nowShow(np); } media("toggle"); };
$("#np-prev").onclick = () => media("prev");
$("#np-next").onclick = () => media("next");
$("#np-bar").onclick = (e) => {
  if (!np?.len) return;
  const r = e.currentTarget.getBoundingClientRect(), at = ((e.clientX - r.left) / r.width) * np.len;
  np.pos = at; npAt = performance.now(); nowFrame();
  media("seek", at);
};

/* volume per app */
let vKeys = "", vDrag = 0;
function volRow(a, master) {
  const ic = master ? `<div class="app-ic">${icon("i-volume")}</div>` : a.key === "system" ? `<div class="app-ic">${icon("i-monitor")}</div>` : appIcon({ icon: a.path, name: a.name });
  return `<div class="row vol${master ? " master" : ""}" data-k="${esc(a.key)}">
    ${ic}
    <div class="main"><div class="name raw" title="${esc(a.name)}">${esc(a.name)}</div></div>
    <input type="range" class="rng" min="0" max="100" step="1">
    <b class="rv"></b>
    <button class="icon-btn mute" title="Mute">${icon("i-volume")}</button>
  </div>`;
}
function volShow(m) {
  $("#v-dev").textContent = m.device || "";
  const all = [{ key: "master", name: tr("All sounds"), volume: m.master, muted: m.muted }, ...m.apps.map((a) => ({ ...a, name: a.key === "system" ? tr(a.name) : a.name }))];
  const keys = all.map((a) => a.key).join("|");
  if (keys !== vKeys) {
    vKeys = keys;
    $("#v-list").innerHTML = all.map((a, i) => volRow(a, i === 0)).join("") + (m.apps.length ? "" : `<div class="empty">No app is playing sound right now.</div>`);
    $("#v-list").querySelectorAll(".vol").forEach((row) => {
      const k = row.dataset.k, r = row.querySelector(".rng");
      r.onpointerdown = () => (vDrag = Date.now());
      r.oninput = () => { vDrag = Date.now(); pct(r); row.querySelector(".rv").textContent = r.value + "%"; volSend(k, { volume: r.value / 100 }); };
      row.querySelector(".mute").onclick = () => { const on = !row.classList.contains("is-muted"); row.classList.toggle("is-muted", on); volMute(row, on); volSend(k, { muted: on }, true); };
    });
    const miss = [...new Set(m.apps.map((a) => a.path).filter((p) => p && !(p in iconCache)))];
    if (miss.length) {
      miss.forEach((p) => (iconCache[p] = null));
      invoke("icons", { sources: miss }).then((got) => {
        Object.assign(iconCache, got);
        document.querySelectorAll("#v-list .app-ic[data-icon]").forEach((el) => { const s = iconCache[el.dataset.icon]; if (s && !el.querySelector("img")) el.innerHTML = `<img src="${s}" alt="">`; });
      }).catch(console.error);
    }
  }
  if (Date.now() - vDrag < 1500) return;        // don't jump while you are dragging
  for (const a of all) {
    const row = document.querySelector(`#v-list .vol[data-k="${CSS.escape(a.key)}"]`);
    if (!row) continue;
    const r = row.querySelector(".rng");
    r.value = Math.round(a.volume * 100); pct(r);
    row.querySelector(".rv").textContent = r.value + "%";
    row.classList.toggle("is-muted", a.muted); volMute(row, a.muted);
    row.querySelector(".name").classList.toggle("idle", a.playing === false);
  }
}
function volMute(row, on) { const b = row.querySelector(".mute"); b.classList.toggle("muted", on); b.innerHTML = icon(on ? "i-mute" : "i-volume"); b.title = on ? "Unmute" : "Mute"; }
const vPend = {};
function volSend(key, v, now) {
  clearTimeout(vPend[key]?.t);
  const go = () => invoke("volume_set", { key, volume: v.volume ?? null, muted: v.muted ?? null }).catch((e) => toast(String(e), true));
  if (now) go(); else vPend[key] = { t: setTimeout(go, 40) };
}
async function volTick(force) { if (!visible("sound") && !force) return; try { volShow(await invoke("volume_list")); } catch (e) { console.error(e); } }
setInterval(volTick, 2000);

/* ================= NOTES ================= */
const NCOL = { graphite: "#a3a8b3", amber: "#f2c14e", mint: "#6fd6a8", sky: "#6cb6ff", rose: "#ff8fab", violet: "#b49cff" };
let ns = null;
function ago(ms) {
  const s = (Date.now() - ms) / 1000;
  if (s < 60) return tr("just now");
  if (s < 3600) return LANG === "de" ? `vor ${Math.floor(s / 60)} Min.` : `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return LANG === "de" ? `vor ${Math.floor(s / 3600)} Std.` : `${Math.floor(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString(LOC, { day: "numeric", month: "short" });
}
function notesShow(s) {
  ns = s;
  $("#n-ghost").classList.toggle("on", s.ghost);
  if (keyWait !== $("#n-key")) $("#n-key").textContent = s.ghost_key || "None";
  if (keyWait !== $("#s-key-notes")) $("#s-key-notes").textContent = s.ghost_key || "None";
  const g = $("#n-grid");
  if (!s.items.length) { g.innerHTML = `<div class="empty">No notes yet. A new note appears at the top right of your screen.</div>`; return; }
  g.innerHTML = s.items.map((n) => {
    const text = n.text.replace(/^\[( |x)\] /gm, (_, x) => (x === "x" ? "☑ " : "☐ ")).trim();
    return `<div class="nc ${n.shown ? "" : "off"}" data-id="${n.id}" style="--nc:${NCOL[n.color] || NCOL.graphite}">
      <div class="nc-text raw ${text ? "" : "empty-t"}">${text ? esc(text) : tr("Empty note")}</div>
      <div class="nc-foot"><span class="when">${ago(n.updated)}</span>
        <div class="nc-colors">${Object.entries(NCOL).map(([k, c]) => `<button data-c="${k}" class="${k === n.color ? "on" : ""}" style="background:${c}" title="${k}"></button>`).join("")}</div>
        <button class="icon-btn vis" title="${n.shown ? "Hide from screen" : "Show on screen"}">${icon(n.shown ? "i-show" : "i-eye")}</button>
        <button class="icon-btn del" title="Delete">${icon("i-trash")}</button></div>
    </div>`;
  }).join("");
  g.querySelectorAll(".nc").forEach((c) => {
    const id = c.dataset.id, n = s.items.find((x) => x.id === id);
    c.onclick = (e) => { if (!e.target.closest("button")) noteSave(id, { shown: true }); };
    c.querySelector(".vis").onclick = () => noteSave(id, { shown: !n.shown });
    c.querySelectorAll(".nc-colors button").forEach((b) => (b.onclick = () => noteSave(id, { color: b.dataset.c })));
    c.querySelector(".del").onclick = async () => {
      const first = n.text.split("\n")[0].slice(0, 40) || tr("Empty note");
      if (!(await confirmBox("Delete this note?", "Notes are not kept in the Recycle Bin, it is gone for good.", [[first, ago(n.updated)]]))) return;
      try { await invoke("note_delete", { id }); } catch (e) { toast(String(e), true); }
      notesLoad();
    };
  });
}
async function notesLoad() { try { notesShow(await invoke("notes_list")); } catch (e) { console.error(e); } }
async function noteSave(id, patch) { try { await invoke("note_save", { id, patch }); } catch (e) { toast(String(e), true); } notesLoad(); }
$("#n-new").onclick = async () => { try { await invoke("note_new"); notesLoad(); } catch (e) { toast(String(e), true); } };
$("#n-ghost").onclick = async () => { if (ns) try { notesShow(await invoke("notes_ghost", { on: !ns.ghost })); } catch (e) { toast(String(e), true); } };
hotkeyCap($("#n-key"), () => ns?.ghost_key, async (key) => { try { notesShow(await invoke("notes_key", { key })); } catch (e) { toast(String(e), true); notesLoad(); } });
hotkeyCap($("#s-key-notes"), () => ns?.ghost_key, async (key) => { try { notesShow(await invoke("notes_key", { key })); } catch (e) { toast(String(e), true); notesLoad(); } });
// the Jarvis key can also be a mouse button, so it is changed in the Jarvis dialog
function jarvisKey() {
  const k = jv.status?.talk || jv.button;
  $("#s-key-jarvis").textContent = jv.status?.installed === false ? tr("Not set up") : talkText(k || "x2");
}
$("#s-key-jarvis").onclick = () => $("#jarvis").onclick();
const _kyes = $("#k-yes").onclick;
$("#k-yes").onclick = async (...a) => { await _kyes?.(...a); jarvisKey(); };
let notesT = null;
T.event.listen("notes", () => { clearTimeout(notesT); notesT = setTimeout(() => current === "notes" && notesLoad(), 300); });
T.event.listen("ghost", ({ payload }) => { if (ns) { ns.ghost = payload; $("#n-ghost").classList.toggle("on", payload); } toast(payload ? "Notes: click-through on" : "Notes: click-through off"); });

/* ================= DOWNLOADS ================= */
const DLCAT = { images: ["Images", "i-spark"], videos: ["Videos", "i-film"], music: ["Music", "i-music"], documents: ["Documents", "i-note"], archives: ["Archives", "i-folder"], installers: ["Installers", "i-down"] };
let dl = null, dlPick = new Set();
function dlShow(d) {
  dl = d;
  $("#dl-auto").classList.toggle("on", d.auto);
  $("#dl-undo").hidden = !d.can_undo;
  const by = {};
  for (const f of d.files) (by[f.cat] ||= []).push(f);
  $("#dl-cats").innerHTML = d.files.length
    ? Object.entries(DLCAT).filter(([k]) => by[k]).map(([k, [n, i]]) => `<span>${icon(i)}${n}<b>${by[k].length}</b></span>`).join("")
    : `<span>${icon("i-check")}Everything is sorted</span>`;
  $("#dl-sort").disabled = !d.files.length;
  const old = $("#dl-old");
  if (!d.old.length) { old.innerHTML = ""; return; }
  dlPick = new Set(d.old.map((f) => f.name).filter((n) => dlPick.size === 0 || dlPick.has(n)));
  old.innerHTML = `<div class="group" style="padding:12px 32px 4px">Old installers, probably already installed</div>
    <div class="list">${d.old.map((f) => `<div class="row pick ${dlPick.has(f.name) ? "sel" : ""}" data-n="${esc(f.name)}"><div class="cb">${icon("i-check")}</div>
      <div class="main"><div class="name raw">${esc(f.name)}</div><div class="sub">${f.days} days old</div></div><div class="size">${gb(f.size)}</div></div>`).join("")}</div>
    <div class="card-foot"><span>Selected <b id="dl-sel"></b></span><button class="btn" id="dl-rm">${icon("i-trash")}Move to Recycle Bin</button></div>`;
  const upd = () => { $("#dl-sel").textContent = gb(d.old.filter((f) => dlPick.has(f.name)).reduce((a, f) => a + f.size, 0)); $("#dl-rm").disabled = !dlPick.size; };
  old.querySelectorAll(".row").forEach((r) => (r.onclick = () => { const n = r.dataset.n; dlPick.has(n) ? dlPick.delete(n) : dlPick.add(n); r.classList.toggle("sel", dlPick.has(n)); upd(); }));
  upd();
  $("#dl-rm").onclick = async () => {
    const sel = d.old.filter((f) => dlPick.has(f.name));
    if (!(await confirmBox("Move to Recycle Bin?", "These will be moved to the Recycle Bin. You can restore them from there.", sel.map((f) => [f.name, gb(f.size)])))) return;
    try { const [freed, st] = await invoke("downloads_recycle", { names: sel.map((f) => f.name) }); dlPick.clear(); dlShow(st); toast(`Freed ${gb(freed)}`); }
    catch (e) { toast(String(e), true); }
  };
}
async function dlLoad() { try { dlShow(await invoke("downloads_state")); } catch (e) { console.error(e); } }
$("#dl-sort").onclick = async () => {
  try { const [n, st] = await invoke("downloads_sort"); dlShow(st); toast(n ? `Sorted ${n} file${n > 1 ? "s" : ""}` : "Nothing to sort"); } catch (e) { toast(String(e), true); }
};
$("#dl-undo").onclick = async () => {
  try { const [n, st] = await invoke("downloads_undo"); dlShow(st); toast(`Put back ${n} file${n === 1 ? "" : "s"}`); } catch (e) { toast(String(e), true); }
};
$("#dl-auto").onclick = async () => { if (dl) try { dlShow(await invoke("downloads_auto", { on: !dl.auto })); } catch (e) { toast(String(e), true); } };
$("#dl-open").onclick = () => invoke("downloads_open");

/* start: the crosshair dot in the sidebar */
crossLoad();
