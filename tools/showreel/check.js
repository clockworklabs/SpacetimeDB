// Automated checks for a release reel, run before rendering (build-release.sh) since nobody reviews stills in the pipeline.
// Built from bugs caught by eye while making the 2.x videos.
//   node --expose-gc check.js release-v2.9 [--strict]
// Errors (exit 1): missing glyphs, text off the canvas, a NaN soundtrack, no "## Script" in DECISIONS.md.
// Warnings: text crossing or touching the edge of a box, text overlapping text, headlines shrunk small, scenes too short to read.
// --strict turns warnings into errors.
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const { W, H, loadAssets, createCanvas } = require('./lib');

const argv = process.argv.slice(2);
const reel = argv.find(a => !a.startsWith('--'));
const strict = argv.includes('--strict');
if (!reel) { console.error('usage: node --expose-gc check.js release-vX.Y [--strict]'); process.exit(2); }
const DIR = path.resolve(__dirname, reel);
const gc = global.gc || (() => {});

const errors = [], warnings = [];
const seen = new Set();
const report = (list, key, msg) => { if (!seen.has(key)) { seen.add(key); list.push(msg); } };

// ---------- instrumentation: record text runs and rounded boxes in device coordinates ----------
function instrument(ctx) {
  const rec = { texts: [], boxes: [] };
  let path = null;
  const tf = (x, y) => { const m = ctx.getTransform(); return [m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f]; };
  const orig = {};
  for (const k of ['fillText', 'beginPath', 'moveTo', 'lineTo', 'arcTo', 'fill', 'rect', 'clip', 'save', 'restore']) orig[k] = ctx[k].bind(ctx);
  // Clip regions (bounding boxes), tracked through save/restore, so clipped text is measured as drawn.
  let clipBox = null; const clipStack = [];
  ctx.save = () => { clipStack.push(clipBox); return orig.save(); };
  ctx.restore = () => { clipBox = clipStack.length ? clipStack.pop() : null; return orig.restore(); };
  ctx.rect = (x, y, w, h) => { path?.pts.push(tf(x, y), tf(x + w, y + h)); return orig.rect(x, y, w, h); };
  ctx.clip = (...a) => {
    if (path && path.pts.length) {
      const xs = path.pts.map(p => p[0]), ys = path.pts.map(p => p[1]);
      const b = { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
      clipBox = clipBox ? { x0: Math.max(clipBox.x0, b.x0), y0: Math.max(clipBox.y0, b.y0), x1: Math.min(clipBox.x1, b.x1), y1: Math.min(clipBox.y1, b.y1) } : b;
    }
    return orig.clip(...a);
  };
  ctx.beginPath = () => { path = { pts: [], arcs: 0 }; return orig.beginPath(); };
  ctx.moveTo = (x, y) => { path?.pts.push(tf(x, y)); return orig.moveTo(x, y); };
  ctx.lineTo = (x, y) => { path?.pts.push(tf(x, y)); return orig.lineTo(x, y); };
  ctx.arcTo = (x1, y1, x2, y2, r) => { if (path) { path.arcs++; path.pts.push(tf(x1, y1), tf(x2, y2)); } return orig.arcTo(x1, y1, x2, y2, r); };
  const closeBox = () => {
    if (path && path.arcs >= 4 && ctx.globalAlpha > 0.5) {
      const xs = path.pts.map(p => p[0]), ys = path.pts.map(p => p[1]);
      const b = { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
      if (b.x1 - b.x0 > 60 && b.y1 - b.y0 > 30) rec.boxes.push(b);
    }
  };
  // Only filled rounded boxes (panels, cards, pills) count; outline-only highlights are meant to wrap part of a line.
  ctx.fill = (...a) => { closeBox(); return orig.fill(...a); };
  ctx.fillText = (s, x, y) => {
    if (s && s.trim()) {
      const size = +(/(\d+(?:\.\d+)?)px/.exec(ctx.font) || [0, 20])[1];
      const wgt = +(/^(\d+)/.exec(ctx.font) || [0, 400])[1];
      const w = ctx.measureText(s).width;
      const al = ctx.textAlign;
      const lx = al === 'center' ? x - w / 2 : al === 'right' || al === 'end' ? x - w : x;
      const [ax, ay] = tf(lx, y - size * 0.72), [bx, by] = tf(lx + w, y + size * 0.2);
      let r = { x0: Math.min(ax, bx), y0: Math.min(ay, by), x1: Math.max(ax, bx), y1: Math.max(ay, by) };
      if (clipBox) r = { x0: Math.max(r.x0, clipBox.x0), y0: Math.max(r.y0, clipBox.y0), x1: Math.min(r.x1, clipBox.x1), y1: Math.min(r.y1, clipBox.y1) };
      if (r.x1 - r.x0 > 1 && r.y1 - r.y0 > 1) rec.texts.push({ s, font: ctx.font, size, wgt, alpha: ctx.globalAlpha, ...r });
    }
    return orig.fillText(s, x, y);
  };
  return rec;
}

// Consecutive per-character draws on one line (riseText, tracked text) become one run.
function runs(texts) {
  const out = [];
  for (const t of texts) {
    const p = out[out.length - 1];
    if (p && p.font === t.font && Math.abs(p.y0 - t.y0) < 2 && t.x0 >= p.x1 - 6 && t.x0 <= p.x1 + t.size * 0.9) { p.s += t.s; p.x1 = t.x1; p.alpha = Math.min(p.alpha, t.alpha); }
    else out.push({ ...t });
  }
  return out;
}

// ---------- glyph check: a character the fonts lack renders the same as U+10FFFF ----------
const glyphCanvas = createCanvas(64, 64), gctx = glyphCanvas.getContext('2d');
const glyphCache = new Map();
function missingGlyph(family, ch) {
  const key = family + '|' + ch;
  if (glyphCache.has(key)) return glyphCache.get(key);
  const draw = c => { gctx.clearRect(0, 0, 64, 64); gctx.fillStyle = '#fff'; gctx.font = `500 40px ${family}`; gctx.fillText(c, 8, 48); return Buffer.from(gctx.getImageData(0, 0, 64, 64).data.buffer).toString('base64'); };
  const res = draw(ch) === draw('\u{10FFFF}');
  glyphCache.set(key, res);
  return res;
}

(async () => {
  await loadAssets();
  const SC = require(path.join(DIR, 'scenes'));
  const DUR = SC.DUR, SCENES = SC.SCENES;
  const canvas = createCanvas(W, H), ctx = canvas.getContext('2d');
  const rec = instrument(ctx);

  // scenes: long enough to read
  SCENES.forEach((s, i) => {
    const end = i + 1 < SCENES.length ? SCENES[i + 1].t : DUR;
    const len = end - s.t;
    if (i > 0 && i < SCENES.length - 1 && len < 6) report(warnings, 'len' + i, `scene "${s.name}" lasts ${len.toFixed(1)} s (headlines need ~2.5 s after they appear; aim for 6.5 s or more)`);
  });

  // settled frames of every scene (skip entries and exits)
  const chars = new Map();
  for (let i = 0; i < SCENES.length; i++) {
    const s0 = SCENES[i].t, s1 = i + 1 < SCENES.length ? SCENES[i + 1].t : DUR;
    for (let t = s0 + 1.2; t < s1 - 0.7; t += 0.5) {
      rec.texts.length = 0; rec.boxes.length = 0;
      SC.frame(ctx, t, t);
      const name = SCENES[i].name, at = `"${name}" at ${t.toFixed(1)} s`;
      for (const tx of rec.texts) {
        const fam = tx.font.replace(/^.*?px\s*/, '');
        for (const ch of tx.s) if (ch.trim()) { if (!chars.has(fam)) chars.set(fam, new Map()); chars.get(fam).set(ch, at); }
      }
      const R = runs(rec.texts).filter(r => r.alpha > 0.6);
      for (const r of R) {
        if (r.x0 < 30 || r.x1 > W - 30 || r.y0 < 20 || r.y1 > H - 20) report(errors, 'off' + r.s, `text off the canvas in ${at}: "${r.s.slice(0, 50)}"`);
        if (r.wgt >= 900 && r.size < 64 && r.size > 30) report(warnings, 'small' + name, `headline shrunk to ${r.size}px in "${name}": shorten it`);
        for (const b of rec.boxes) {
          const inside = r.x0 >= b.x0 - 1 && r.x1 <= b.x1 + 1 && r.y0 >= b.y0 - 1 && r.y1 <= b.y1 + 1;
          const hits = r.x1 > b.x0 && r.x0 < b.x1 && r.y1 > b.y0 && r.y0 < b.y1;
          if (hits && !inside) {
            const ox = Math.min(r.x1, b.x1) - Math.max(r.x0, b.x0), oy = Math.min(r.y1, b.y1) - Math.max(r.y0, b.y0);
            if (ox > 4 && oy > 3) report(warnings, 'edge' + name + r.s, `text crosses a box edge in ${at}: "${r.s.slice(0, 50)}"`);
          } else if (inside && b.y1 - b.y0 > 80 && (b.y1 - r.y1 < 10 || b.x1 - r.x1 < 8)) {
            report(warnings, 'pad' + name + r.s, `text touches the edge of its box in ${at}: "${r.s.slice(0, 50)}"`);
          }
        }
      }
      for (let a = 0; a < R.length; a++) for (let b = a + 1; b < R.length; b++) {
        const p = R[a], q = R[b];
        const ox = Math.min(p.x1, q.x1) - Math.max(p.x0, q.x0), oy = Math.min(p.y1, q.y1) - Math.max(p.y0, q.y0);
        if (ox > 6 && oy > 0.3 * Math.min(p.y1 - p.y0, q.y1 - q.y0)) report(warnings, 'ov' + p.s + q.s, `text overlaps text in ${at}: "${p.s.slice(0, 40)}" / "${q.s.slice(0, 40)}"`);
      }
      gc();
    }
  }
  for (const [fam, m] of chars) for (const [ch, at] of m) if (missingGlyph(fam, ch)) report(errors, 'gl' + fam + ch, `missing glyph "${ch}" (U+${ch.codePointAt(0).toString(16).toUpperCase()}) in ${fam.split(',')[0]}, first in ${at}`);

  // soundtrack
  try {
    const out = execFileSync('node', [path.join(DIR, 'audio.js')]).toString();
    if (/peak NaN/.test(out)) errors.push('soundtrack is NaN (check audio.js indexes and times)');
  } catch (e) { errors.push('audio.js failed: ' + e.message.split('\n')[0]); }

  // decisions
  const dec = path.join(DIR, 'DECISIONS.md');
  if (!fs.existsSync(dec) || !/^## Script/m.test(fs.readFileSync(dec, 'utf8'))) errors.push('DECISIONS.md is missing its "## Script" section');

  for (const e of errors) console.log('ERROR   ' + e);
  for (const w of warnings) console.log('WARNING ' + w);
  const fail = errors.length || (strict && warnings.length);
  console.log(`${reel}: ${errors.length} error(s), ${warnings.length} warning(s)`);
  process.exit(fail ? 1 : 0);
})();
