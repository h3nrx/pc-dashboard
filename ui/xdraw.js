// Draws the crosshair as SVG; used by the preview in the app and by the overlay window.
function crossSVG(c, W) {
  const m = W / 2, g = +c.gap, L = +c.size, t = +c.thick;
  const d = (k) => Math.round(k * 100) / 100;
  let dirs = [[0, -1], [0, 1], [-1, 0], [1, 0]];
  if (c.shape === "t") dirs = dirs.slice(1);
  if (c.shape === "x") dirs = [[-0.7071, -0.7071], [0.7071, -0.7071], [-0.7071, 0.7071], [0.7071, 0.7071]];
  const lines = (stroke, w) => dirs.map(([x, y]) =>
    `<line x1="${d(m + x * g)}" y1="${d(m + y * g)}" x2="${d(m + x * (g + L))}" y2="${d(m + y * (g + L))}" stroke="${stroke}" stroke-width="${w}"/>`).join("");
  const dot = (fill, r) => `<circle cx="${m}" cy="${m}" r="${d(r)}" fill="${fill}"/>`;
  const ring = (stroke, w) => `<circle cx="${m}" cy="${m}" r="${d(L)}" fill="none" stroke="${stroke}" stroke-width="${w}"/>`;
  const shape = (col, extra) => {
    if (c.shape === "dot") return dot(col, Math.max(t, 1.5) + extra / 2);
    return (c.shape === "circle" ? ring(col, t + extra) : lines(col, t + extra)) + (c.dot ? dot(col, t * 0.75 + extra / 2) : "");
  };
  return `<g opacity="${c.opacity}" shape-rendering="${c.shape === "x" || c.shape === "circle" || c.shape === "dot" ? "geometricPrecision" : "crispEdges"}">` +
    (c.outline ? `<g opacity=".85">${shape("#000", 2)}</g>` : "") + shape(c.color, 0) + "</g>";
}
