// Shared helpers: easing, text, shapes, fonts, assets.
const { createCanvas, GlobalFonts, loadImage, Path2D } = require('@napi-rs/canvas');

const W = 1920, H = 1080, FPS = 60, DUR = 90;

// Brand gradient stops (used only via brandGrad — one element per screen).
const BRAND = { pink: '#ff80fb', purple: '#a880ff', green: '#4cf490' };
// Restrained palette: green is the only accent (red for errors/limits);
// the other legacy accent slots map to neutrals.
const C = {
  bg: '#0B1114',
  white: '#F4F6FC',
  green: '#4cf490',
  pink: '#F4F6FC',
  purple: '#9aa3b2',
  yellow: '#F4F6FC',
  red: '#ff4c6a',
  cyan: '#c9d1dc',
};

// ---------- fonts ----------
const G = __dirname + '/node_modules/geist/dist/fonts/';
for (const w of ['Thin', 'Light', 'Regular', 'Medium', 'SemiBold', 'Bold', 'Black', 'UltraBlack']) {
  try { GlobalFonts.registerFromPath(G + `geist-sans/Geist-${w}.ttf`, 'Geist'); } catch (e) {}
  try { GlobalFonts.registerFromPath(G + `geist-mono/GeistMono-${w}.ttf`, 'GeistMono'); } catch (e) {}
}
const FS = __dirname + '/node_modules/@fontsource/';
for (const w of [400, 500, 600, 700, 800, 900]) {
  GlobalFonts.registerFromPath(FS + `inter/files/inter-latin-${w}-normal.woff2`, 'Inter');
  GlobalFonts.registerFromPath(FS + `inter/files/inter-greek-${w}-normal.woff2`, 'InterGreek');
  const m = Math.min(w, 800);
  GlobalFonts.registerFromPath(FS + `source-code-pro/files/source-code-pro-latin-${m}-normal.woff2`, 'SCP');
  GlobalFonts.registerFromPath(FS + `source-code-pro/files/source-code-pro-greek-${m}-normal.woff2`, 'SCPGreek');
}
// brand fonts first; Geist only as glyph fallback (→ etc.)
const SANS = 'Inter, InterGreek, Geist', MONO = 'SCP, SCPGreek, GeistMono';

// ---------- math / easing ----------
const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;
const P = (t, a, b) => clamp((t - a) / (b - a));
const E = {
  lin: t => t,
  outExpo: t => (t >= 1 ? 1 : 1 - Math.pow(2, -10 * t)),
  inExpo: t => (t <= 0 ? 0 : Math.pow(2, 10 * t - 10)),
  inOutExpo: t => (t <= 0 ? 0 : t >= 1 ? 1 : t < 0.5 ? Math.pow(2, 20 * t - 10) / 2 : (2 - Math.pow(2, -20 * t + 10)) / 2),
  outCubic: t => 1 - Math.pow(1 - t, 3),
  inCubic: t => t * t * t,
  inOutCubic: t => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2),
  outQuart: t => 1 - Math.pow(1 - t, 4),
  inQuart: t => t * t * t * t,
  inOutQuart: t => (t < 0.5 ? 8 * t * t * t * t : 1 - Math.pow(-2 * t + 2, 4) / 2),
  outBack: (t, s = 1.70158) => 1 + (s + 1) * Math.pow(t - 1, 3) + s * Math.pow(t - 1, 2),
  inBack: (t, s = 1.70158) => (s + 1) * t * t * t - s * t * t,
  outSine: t => Math.sin((t * Math.PI) / 2),
  inOutSine: t => -(Math.cos(Math.PI * t) - 1) / 2,
};
// damped spring 0 -> 1 with overshoot
const spring = (t, f = 2.2, d = 7) => (t <= 0 ? 0 : 1 - Math.exp(-d * t) * Math.cos(f * 2 * Math.PI * t));
// decaying pulse 1 -> 0
const pulse = (t, k = 6) => (t < 0 ? 0 : Math.exp(-k * t));

function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const hash = n => { const r = rng(n * 9301 + 49297); r(); return r(); };

function hexA(hex, a) {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

// ---------- shapes ----------
function rr(ctx, x, y, w, h, r) {
  r = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function glass(ctx, x, y, w, h, r = 18, o = {}) {
  ctx.save();
  rr(ctx, x, y, w, h, r);
  ctx.fillStyle = o.fill || '#121A1F';
  ctx.fill();
  ctx.lineWidth = 1.5;
  ctx.strokeStyle = o.stroke0 || 'rgba(244,246,252,0.12)';
  ctx.stroke();
  ctx.restore();
}

function brandGrad(ctx, x0, y0, x1, y1, shift = 0) {
  const g = ctx.createLinearGradient(x0, y0, x1, y1);
  const stops = [BRAND.pink, BRAND.purple, BRAND.green];
  g.addColorStop(0, stops[0]);
  g.addColorStop(clamp(0.5 + shift), stops[1]);
  g.addColorStop(1, stops[2]);
  return g;
}

// ---------- text ----------
function font(ctx, weight, size, fam = SANS) { ctx.font = `${weight} ${size}px ${fam}`; }

// Per-letter rising reveal inside a baseline mask.
// o: {t (time since start), stagger, dur, align, fill, rise, mask, blurIn, tracking, fam, weight, size}
function riseText(ctx, str, x, y, o) {
  const size = o.size, weight = o.weight ?? 900, fam = o.fam ?? SANS;
  font(ctx, weight, size, fam);
  const tracking = o.tracking ?? 0;
  const chars = [...str];
  const widths = chars.map(ch => ctx.measureText(ch).width + tracking);
  const total = widths.reduce((a, b) => a + b, 0) - tracking;
  let sx = x;
  if (o.align === 'center') sx = x - total / 2;
  else if (o.align === 'right') sx = x - total;
  const stagger = o.stagger ?? 0.02, dur = o.dur ?? 0.5;
  const rise = o.rise ?? size * 1.05;
  ctx.save();
  if (o.mask !== false) {
    ctx.beginPath();
    ctx.rect(sx - size, y - size * 1.0, total + size * 2, size * 1.28);
    ctx.clip();
  }
  ctx.textBaseline = 'alphabetic';
  let cx = sx;
  const fill = typeof o.fill === 'function' ? o.fill(sx, total) : o.fill || C.white;
  ctx.fillStyle = fill;
  if (o.glow) { ctx.shadowColor = o.glow; ctx.shadowBlur = o.glowBlur ?? 30; }
  for (let i = 0; i < chars.length; i++) {
    const idx = o.reverse ? chars.length - 1 - i : i;
    const p = (o.ease || E.outExpo)(P(o.t, idx * stagger, idx * stagger + dur));
    const out = o.outT != null ? E.inExpo(P(o.outT, idx * (o.outStagger ?? stagger * 0.6), idx * (o.outStagger ?? stagger * 0.6) + (o.outDur ?? 0.3))) : 0;
    const dy = (1 - p) * rise - out * rise * (o.outDir ?? 1);
    if (p > 0 && out < 1) {
      const ga = ctx.globalAlpha;
      ctx.globalAlpha = ga * (o.fade === false ? 1 : clamp(p * 1.5) * (1 - out));
      ctx.fillText(chars[i], cx, y + dy);
      ctx.globalAlpha = ga;
    }
    cx += widths[i];
  }
  ctx.restore();
  return { x: sx, w: total };
}

function measure(ctx, str, weight, size, fam = SANS, tracking = 0) {
  font(ctx, weight, size, fam);
  return [...str].reduce((a, ch) => a + ctx.measureText(ch).width + tracking, 0) - tracking;
}

// Plain text with tracking
function text(ctx, str, x, y, o = {}) {
  font(ctx, o.weight ?? 500, o.size ?? 20, o.fam ?? MONO);
  const tr = o.tracking ?? 0;
  ctx.save();
  ctx.fillStyle = o.fill || C.white;
  if (o.alpha != null) ctx.globalAlpha *= o.alpha;
  if (o.glow) { ctx.shadowColor = o.glow; ctx.shadowBlur = o.glowBlur ?? 20; }
  ctx.textBaseline = o.baseline || 'alphabetic';
  if (!tr) {
    ctx.textAlign = o.align || 'left';
    ctx.fillText(str, x, y);
  } else {
    const w = measure(ctx, str, o.weight ?? 500, o.size ?? 20, o.fam ?? MONO, tr);
    let cx = o.align === 'center' ? x - w / 2 : o.align === 'right' ? x - w : x;
    font(ctx, o.weight ?? 500, o.size ?? 20, o.fam ?? MONO);
    for (const ch of str) { ctx.fillText(ch, cx, y); cx += ctx.measureText(ch).width + tr; }
  }
  ctx.restore();
}

// Scrambled decode text effect
const GLYPHS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789#$%&*+=<>/\\';
function scramble(str, p, seed) {
  const n = str.length;
  let out = '';
  for (let i = 0; i < n; i++) {
    const ch = str[i];
    const reveal = p * (n + 6) - i;
    if (reveal >= 6 || ch === ' ') out += ch;
    else if (reveal > 0) out += GLYPHS[Math.floor(hash(seed + i * 31 + Math.floor(reveal * 8)) * GLYPHS.length)];
    else out += '';
  }
  return out;
}

// ---------- logo ----------
const LOGO_D = 'M514.832 308.261C510.567 208.836 529.176 151.399 622 45L470.805 197.793L457.45 211.29C445.419 223.447 444.68 242.492 452.167 257.87C478.81 312.596 469.606 380.633 424.553 426.162C379.691 471.498 312.743 480.913 258.708 454.407C243.11 446.756 223.705 447.505 211.485 459.854L198.573 472.903L198.7 473.01L144.099 528.589C185.326 504.458 248.988 511.382 287.392 515.558C298.029 516.715 306.728 517.661 312.476 517.678C365.084 520.293 418.548 501.3 458.727 460.697C500.074 418.912 518.776 362.908 514.832 308.261ZM326.524 122.322C332.272 122.339 340.971 123.285 351.608 124.442C390.012 128.619 453.674 135.542 494.901 111.412L440.3 166.99L440.427 167.097L427.515 180.146C415.295 192.495 395.89 193.245 380.292 185.593C326.257 159.087 259.309 168.502 214.447 213.838C169.394 259.367 160.19 327.404 186.833 382.131C194.32 397.509 193.581 416.553 181.55 428.71L168.195 442.207L17 595C109.824 488.601 128.433 431.164 124.168 331.74C120.224 277.092 138.926 221.088 180.273 179.304C220.452 138.701 273.916 119.707 326.524 122.322Z';
const logoPath = new Path2D(LOGO_D);

// Draw the SpacetimeDB mark centered at cx,cy with overall size (px).
// o: {sweep 0..1 (swoosh reveal), ring 0..1, core 0..1, glow, rot, color}
function drawMark(ctx, cx, cy, size, o = {}) {
  const s = size / 640;
  const sweep = o.sweep ?? 1, ring = o.ring ?? 1;
  ctx.save();
  ctx.translate(cx, cy);
  if (o.rot) ctx.rotate(o.rot);
  ctx.scale(s, s);
  ctx.translate(-319.5, -320);
  const col = o.color || C.white;
  if (o.glow) { ctx.shadowColor = o.glow; ctx.shadowBlur = (o.glowBlur ?? 40) * 1; }
  // swooshes, revealed by a rotating wedge
  if (sweep > 0) {
    ctx.save();
    if (sweep < 1) {
      ctx.beginPath();
      ctx.moveTo(319.5, 320);
      const a0 = -Math.PI * 0.75;
      ctx.arc(319.5, 320, 700, a0, a0 + sweep * Math.PI * 2);
      ctx.closePath();
      ctx.clip();
    }
    ctx.fillStyle = col;
    ctx.fill(logoPath);
    ctx.restore();
  }
  // outer ring (two circles in the original: r≈125 and r≈117 + stroked 119.75 r)
  if (ring > 0) {
    ctx.strokeStyle = col;
    ctx.lineWidth = 8;
    ctx.beginPath();
    ctx.arc(319.5, 320, 119.75, -Math.PI / 2, -Math.PI / 2 + ring * Math.PI * 2);
    ctx.stroke();
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.arc(319.5, 320, 124.5, Math.PI / 2, Math.PI / 2 + ring * Math.PI * 2);
    ctx.stroke();
  }
  ctx.restore();
}

// ---------- assets ----------
const A = {};
async function loadAssets() {
  // Logos and wordmark are bundled in ./assets (copied from the SpacetimeDB repo's docs/static/images/logos and images/dark).
  const L = __dirname + '/assets/logos/';
  const names = ['rust', 'react', 'unity', 'unreal', 'svelte', 'vue', 'nextjs', 'nodejs', 'bun', 'deno', 'angular', 'tanstack', 'remix', 'nuxt', 'typescript', 'cpp', 'javascript', 'html5', 'csharp'];
  A.logos = {};
  for (const n of names) {
    const im = await loadImage(L + n + '-logo.svg');
    // tint to white on an offscreen canvas
    const S = 160;
    const c = createCanvas(S, S), x = c.getContext('2d');
    const k = Math.min(S / im.width, S / im.height);
    const w = im.width * k, h = im.height * k;
    x.drawImage(im, (S - w) / 2, (S - h) / 2, w, h);
    if (!['typescript', 'cpp', 'javascript', 'csharp'].includes(n)) {
      x.globalCompositeOperation = 'source-in';
      x.fillStyle = C.white;
      x.fillRect(0, 0, S, S);
    }
    A.logos[n] = c;
  }
  const wm = await loadImage(__dirname + '/assets/logo-text.svg');
  const wc = createCanvas(348 * 4, 53 * 4), wx = wc.getContext('2d');
  wx.drawImage(wm, 0, 0, 348 * 4, 53 * 4);
  A.wordmark = wc;
  return A;
}

// Brand rule: no outer glows. Make shadowBlur a no-op on every 2D context.
function noGlow(ctx) {
  Object.defineProperty(ctx, 'shadowBlur', { get: () => 0, set: () => {}, configurable: true });
  return ctx;
}
const _createCanvas = createCanvas;
function createCanvasNG(w, h) {
  const c = _createCanvas(w, h);
  const gc = c.getContext.bind(c);
  c.getContext = (...a) => noGlow(gc(...a));
  return c;
}

module.exports = {
  W, H, FPS, DUR, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, rng, hash, hexA,
  rr, glass, brandGrad, font, riseText, measure, text, scramble, drawMark, logoPath, A, loadAssets, createCanvas: createCanvasNG, BRAND,
};
