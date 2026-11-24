// Looks: background picture, colours and a light animation behind the cards.
const LOOKS = [
  ["chrome", "Chrome", "Grey glass"], ["winter", "Winter", "Snow and fairy lights"], ["midnight", "Midnight", "Milky Way"],
  ["ocean", "Ocean", "Under water"], ["sakura", "Sakura", "Falling petals"], ["sunset", "Sunset", "Evening sky"],
];
const look = { name: null, fx: true, parts: [], raf: 0, last: 0 };

function setLook(name, fx) {
  if (!LOOKS.some((l) => l[0] === name)) name = "chrome";
  if (look.name === name && look.fx === fx) return;
  look.name = name; look.fx = fx;
  const root = document.documentElement;
  root.dataset.look = name;
  root.classList.toggle("nofx", !fx);
  document.querySelectorAll("#look-grid .look").forEach((b) => b.classList.toggle("on", b.dataset.look === name));
  if (name === "winter" && fx) lights(); else document.querySelector("#lights").innerHTML = "";
  look.parts = spawn();
  cancelAnimationFrame(look.raf);
  draw.ctx.clearRect(0, 0, 1180, 760);
  if (look.parts.length) look.raf = requestAnimationFrame(draw);
}

/* a string of fairy lights along the top edge */
function lights() {
  const n = 23, w = 1180, y = (x) => 6 + 9 * Math.abs(Math.sin((x / w) * Math.PI * 6));
  let path = "";
  for (let x = 0; x <= w; x += 8) path += (x ? "L" : "M") + x + "," + y(x).toFixed(1);
  const cols = ["#ff5a4f", "#ffd166", "#58d68d", "#5fb8ff", "#ff8ad1"];
  let s = `<svg viewBox="0 0 ${w} 40" preserveAspectRatio="none"><path d="${path}"/>`;
  for (let i = 0; i < n; i++) {
    const x = ((i + 0.5) / n) * w;
    s += `<ellipse cx="${x.toFixed(1)}" cy="${(y(x) + 7).toFixed(1)}" rx="3.6" ry="5" fill="${cols[i % 5]}" style="color:${cols[i % 5]};animation-delay:${-(i * 0.37) % 2.4}s"/>`;
  }
  document.querySelector("#lights").innerHTML = s + "</svg>";
}

const rnd = (a, b) => a + Math.random() * (b - a);
function spawn() {
  if (!look.fx) return [];
  const many = { winter: 90, midnight: 70, ocean: 26, sakura: 24, sunset: 22 }[look.name] || 0;
  return Array.from({ length: many }, () => {
    const p = { x: rnd(0, 1180), y: rnd(0, 760), t: rnd(0, 7), k: rnd(0.3, 1) };
    if (look.name === "midnight") p.y = rnd(0, 480);
    return p;
  });
}
const draw = (now) => {
  look.raf = requestAnimationFrame(draw);
  // 30 pictures a second are plenty, and none while you are in another window
  if (now - look.last < 32 || !document.hasFocus()) return;
  const dt = Math.min(0.06, (now - look.last) / 1000); look.last = now;
  const c = draw.ctx; c.clearRect(0, 0, 1180, 760);
  for (const p of look.parts) {
    p.t += dt;
    if (look.name === "winter") {
      p.y += dt * (18 + p.k * 46); p.x += dt * Math.sin(p.t * 0.8 + p.k * 9) * 14;
      if (p.y > 766) { p.y = -6; p.x = rnd(0, 1180); }
      c.globalAlpha = 0.25 + p.k * 0.55; c.fillStyle = "#fff";
      c.beginPath(); c.arc(p.x, p.y, 0.8 + p.k * 1.9, 0, 7); c.fill();
    } else if (look.name === "midnight") {
      c.globalAlpha = Math.max(0, Math.sin(p.t * (0.5 + p.k)) * 0.8); c.fillStyle = "#dfe8ff";
      c.beginPath(); c.arc(p.x, p.y, 0.6 + p.k * 0.9, 0, 7); c.fill();
    } else if (look.name === "ocean") {
      p.y -= dt * (14 + p.k * 34); p.x += dt * Math.sin(p.t * 1.4 + p.k * 9) * 10;
      if (p.y < -10) { p.y = 770; p.x = rnd(0, 1180); }
      c.globalAlpha = 0.16 + p.k * 0.25; c.strokeStyle = "#d8f6ff"; c.lineWidth = 1;
      c.beginPath(); c.arc(p.x, p.y, 1.5 + p.k * 5, 0, 7); c.stroke();
    } else if (look.name === "sakura") {
      p.y += dt * (16 + p.k * 26); p.x += dt * (12 + Math.sin(p.t + p.k * 9) * 22);
      if (p.y > 770) { p.y = -10; p.x = rnd(-100, 1180); }
      if (p.x > 1190) p.x = -10;
      c.save(); c.translate(p.x, p.y); c.rotate(p.t * (0.4 + p.k)); c.scale(1, 0.45 + 0.4 * Math.abs(Math.sin(p.t * 1.3)));
      c.globalAlpha = 0.45 + p.k * 0.4; c.fillStyle = "#ffc4d8";
      const r = 3.5 + p.k * 4;
      c.beginPath(); c.moveTo(0, -r); c.bezierCurveTo(r, -r * 0.6, r * 0.8, r * 0.7, 0, r); c.bezierCurveTo(-r * 0.8, r * 0.7, -r, -r * 0.6, 0, -r); c.fill();
      c.restore();
    } else {
      p.y -= dt * (5 + p.k * 14); p.x += dt * Math.sin(p.t * 0.7 + p.k * 9) * 8;
      if (p.y < -8) { p.y = 768; p.x = rnd(0, 1180); }
      c.globalAlpha = 0.25 + 0.4 * Math.abs(Math.sin(p.t * 0.9)); c.fillStyle = "#ffd9a8";
      c.beginPath(); c.arc(p.x, p.y, 0.8 + p.k * 1.3, 0, 7); c.fill();
    }
  }
  c.globalAlpha = 1;
};
draw.ctx = document.querySelector("#fx").getContext("2d");
document.querySelector("#look-grid").innerHTML = LOOKS.map(([id, name, desc]) =>
  `<button class="look" data-look="${id}"><span class="lk-pic"${id === "chrome" ? "" : ` style="background-image:url(bg/${id}.jpg)"`}></span><b>${name}</b><span class="lk-d">${desc}</span></button>`).join("");
