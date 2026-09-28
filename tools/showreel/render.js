// Usage:
//   node render.js --still 1.2 4.6 ...        -> out/still_<t>.png
//   node render.js --worker a b seg.mp4       -> frames [a,b) to seg
//   node render.js [--jobs N] [--samples S]    -> full render + mux
const fs = require('fs');
const path = require('path');
const { spawn, execFileSync } = require('child_process');
const { W, H, FPS, DUR, loadAssets, createCanvas } = require('./lib');
const { frame } = require('./scenes');

const OUT = path.join(__dirname, 'out');
fs.mkdirSync(OUT, { recursive: true });
const argv = process.argv.slice(2);
const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
const SAMPLES = +opt('--samples', 5);
const SHUTTER = 0.5; // 180°

function renderMB(ctx, t, acc) {
  if (SAMPLES <= 1) { frame(ctx, t); return ctx.getImageData(0, 0, W, H).data; }
  acc.fill(0);
  for (let s = 0; s < SAMPLES; s++) {
    const ts = t + ((s + 0.5) / SAMPLES - 0.5) * (SHUTTER / FPS);
    frame(ctx, Math.max(0, ts));
    const d = ctx.getImageData(0, 0, W, H).data;
    for (let i = 0; i < d.length; i++) acc[i] += d[i];
  }
  const out = new Uint8ClampedArray(W * H * 4);
  const inv = 1 / SAMPLES;
  for (let i = 0; i < out.length; i++) out[i] = acc[i] * inv + 0.5;
  return out;
}

(async () => {
  await loadAssets();
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');
  const acc = new Uint32Array(W * H * 4);

  if (argv[0] === '--still') {
    const times = argv.slice(1, argv.includes('--samples') ? argv.indexOf('--samples') : undefined);
    for (const ts of times) {
      const t = +ts;
      const t0 = Date.now();
      const px = renderMB(ctx, t, acc);
      const img = ctx.createImageData(W, H);
      img.data.set(px);
      ctx.putImageData(img, 0, 0);
      const f = path.join(OUT, `still_${t.toFixed(2)}.png`);
      fs.writeFileSync(f, canvas.toBuffer('image/png'));
      console.log(f, Date.now() - t0, 'ms');
    }
    return;
  }

  if (argv[0] === '--worker') {
    const a = +argv[1], b = +argv[2], seg = argv[3];
    const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'rawvideo', '-pix_fmt', 'rgba', '-s', `${W}x${H}`, '-r', String(FPS), '-i', '-',
      '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '8', '-threads', '1', '-pix_fmt', 'yuv420p', seg], { stdio: ['pipe', 'inherit', 'inherit'] });
    for (let f = a; f < b; f++) {
      const px = renderMB(ctx, f / FPS, acc);
      const buf = Buffer.from(px.buffer, px.byteOffset, px.byteLength);
      if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once('drain', r));
      if (process.send && (f - a) % 10 === 0) process.send({ done: f - a });
    }
    ff.stdin.end();
    await new Promise(r => ff.on('close', r));
    if (process.send) process.send({ fin: true });
    process.exit(0);
  }

  // orchestrate
  // Keep the machine usable: default to a third of the cores, run workers at low priority.
  const JOBS = +opt('--jobs', Math.max(1, Math.floor(require('os').cpus().length / 3)));
  const total = Math.round(DUR * FPS);
  const per = Math.ceil(total / JOBS);
  const segs = [];
  const t0 = Date.now();
  const prog = new Array(JOBS).fill(0);
  await Promise.all(Array.from({ length: JOBS }, (_, j) => new Promise((res, rej) => {
    const a = j * per, b = Math.min(total, a + per);
    const seg = path.join(OUT, `seg_${String(j).padStart(2, '0')}.mp4`);
    segs.push(seg);
    const { fork } = require('child_process');
    const w = fork(__filename, ['--worker', a, b, seg, '--samples', SAMPLES]);
    try { require('os').setPriority(w.pid, 19); } catch (e) {}
    w.on('message', m => {
      if (m.done != null) prog[j] = m.done;
      const d = prog.reduce((x, y) => x + y, 0);
      if (m.done % 50 === 0) process.stdout.write(`\r${d}/${total} frames  ${((Date.now() - t0) / 1000).toFixed(0)}s   `);
    });
    w.on('exit', c => (c === 0 ? res() : rej(new Error('worker ' + j + ' failed ' + c))));
  })));
  segs.sort();
  fs.writeFileSync(path.join(OUT, 'list.txt'), segs.map(s => `file '${s}'`).join('\n'));
  console.log('\nrender done', (Date.now() - t0) / 1000, 's');
})();
