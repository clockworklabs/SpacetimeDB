// Usage:
//   node render.js --still 1.2 4.6 ...        -> out/still_<t>.png
//   node render.js --worker a b seg.mp4       -> frames [a,b) to seg
//   node render.js [--jobs N] [--samples S]    -> full render (then ./mux.sh)
//   node render.js --slow 2                    -> slowed-down kiosk render into out-kiosk/ (then ./mux-kiosk.sh)
//   node render.js --reel release-v2.10 ...    -> render another reel (its scenes.js, DUR and out/ live in that dir)
const fs = require('fs');
const path = require('path');
const { spawn, execFileSync } = require('child_process');
const { W, H, FPS, loadAssets, createCanvas } = require('./lib');

const argv = process.argv.slice(2);
const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
// --reel DIR renders DIR/scenes.js (which exports its own DUR) into DIR/out; default is the showreel.
const REEL = opt('--reel', null);
const REEL_DIR = REEL ? path.resolve(__dirname, REEL) : __dirname;
const SC = require(path.join(REEL_DIR, 'scenes'));
const { frame, setSlow } = SC;
const DUR = SC.DUR ?? require('./lib').DUR;
// --slow N plays the whole timeline N× slower (kiosk/loop version); it renders into its own dir.
const SLOW = +opt('--slow', 1);
setSlow(SLOW);
const OUT = path.join(REEL_DIR, SLOW === 1 ? 'out' : 'out-kiosk');
fs.mkdirSync(OUT, { recursive: true });
const SAMPLES = +opt('--samples', 5);
const SHUTTER = 0.5; // 180°
// getImageData() returns ~8 MB of native memory per call that V8 doesn't account for,
// so without explicit collection each worker grows by ~500 MB/s. Workers run with
// --expose-gc and collect after every frame.
const gc = global.gc || (() => {});
let OUTBUF = null;

// t = scene time, rt = real time. The shutter spans SHUTTER/FPS of real time, i.e. /SLOW in scene time.
function renderMB(ctx, t, acc, rt = t) {
  if (SAMPLES <= 1) { frame(ctx, t, rt); return ctx.getImageData(0, 0, W, H).data; }
  acc.fill(0);
  for (let s = 0; s < SAMPLES; s++) {
    const k = ((s + 0.5) / SAMPLES - 0.5) * (SHUTTER / FPS);
    frame(ctx, Math.max(0, t + k / SLOW), Math.max(0, rt + k));
    const d = ctx.getImageData(0, 0, W, H).data;
    for (let i = 0; i < d.length; i++) acc[i] += d[i];
  }
  const out = OUTBUF || (OUTBUF = new Uint8ClampedArray(W * H * 4));
  const inv = 1 / SAMPLES;
  for (let i = 0; i < out.length; i++) out[i] = acc[i] * inv + 0.5;
  return out;
}

(async () => {
  await loadAssets();
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');
  const acc = new Uint32Array(W * H * 4);

  if (argv.includes('--still')) {
    const times = argv.slice(argv.indexOf('--still') + 1);
    const end = times.findIndex(a => a.startsWith('--'));
    if (end >= 0) times.length = end;
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
      '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '8', '-threads', '4', '-pix_fmt', 'yuv420p', seg], { stdio: ['pipe', 'inherit', 'inherit'] });
    for (let f = a; f < b; f++) {
      const px = renderMB(ctx, f / (FPS * SLOW), acc, f / FPS);
      const buf = Buffer.from(px.buffer, px.byteOffset, px.byteLength);
      if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once('drain', r));
      else await new Promise(r => setImmediate(r));
      gc();
      if (process.send && (f - a) % 10 === 0) process.send({ done: f - a });
    }
    ff.stdin.end();
    await new Promise(r => ff.on('close', r));
    if (process.send) process.send({ fin: true });
    process.exit(0);
  }

  // orchestrate
  // Workers render short chunks and then exit, so any native memory the canvas library
  // leaks is returned to the OS. Memory per worker stays under ~600 MB; peak ≈ JOBS × 600 MB.
  const JOBS = +opt('--jobs', Math.max(1, require('os').cpus().length - 2));
  const CHUNK = +opt('--chunk', 90);
  const total = Math.round(DUR * SLOW * FPS);
  const chunks = [];
  for (let a = 0; a < total; a += CHUNK) chunks.push([a, Math.min(total, a + CHUNK)]);
  const segs = chunks.map((_, j) => path.join(OUT, `seg_${String(j).padStart(3, '0')}.mp4`));
  const t0 = Date.now();
  let next = 0, doneFrames = 0;
  const { fork } = require('child_process');
  const runOne = () => new Promise((res, rej) => {
    const j = next++;
    if (j >= chunks.length) return res(false);
    const [a, b] = chunks[j];
    const w = fork(__filename, ['--worker', a, b, segs[j], '--samples', SAMPLES, '--slow', SLOW, ...(REEL ? ['--reel', REEL] : [])], { execArgv: ['--expose-gc', '--max-old-space-size=512'] });
    w.on('exit', c => {
      if (c !== 0) return rej(new Error(`chunk ${j} failed (${c})`));
      doneFrames += b - a;
      process.stdout.write(`\r${doneFrames}/${total} frames  ${((Date.now() - t0) / 1000).toFixed(0)}s   `);
      res(true);
    });
  });
  const lane = async () => { while (await runOne()); };
  await Promise.all(Array.from({ length: JOBS }, lane));
  fs.writeFileSync(path.join(OUT, 'list.txt'), segs.map(s => `file '${s}'`).join('\n'));
  fs.writeFileSync(path.join(OUT, 'slow.txt'), String(SLOW));
  console.log('\nrender done', (Date.now() - t0) / 1000, 's');
})();
