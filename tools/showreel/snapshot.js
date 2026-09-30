// Regression snapshot for every reel: hashes rendered frames (every --step s from --start, default 0.25 from 0.37; 1 sample) and each soundtrack.
// Use it around any change to shared code: the output must stay byte-identical.
//   node --expose-gc snapshot.js --save    → out/snapshot.json
//   node --expose-gc snapshot.js --check   → compares with out/snapshot.json, exits 1 on any difference
//   node --expose-gc snapshot.js --check release-v2.9   (limit to some reels)
// Hashes are machine-specific (fonts, canvas build); compare only on the same machine.
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');
const { W, H, loadAssets, createCanvas } = require('./lib');

const argv = process.argv.slice(2);
const mode = argv.includes('--save') ? 'save' : 'check';
const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? +argv[i + 1] : d; };
// Off-grid sampling (scenes start on x.0/x.5 s), dense enough to catch timing changes.
const STEP = opt('--step', 0.25), START = opt('--start', 0.37);
const only = argv.filter((a, i) => !a.startsWith('--') && !['--step', '--start'].includes(argv[i - 1]));
const FILE = path.join(__dirname, 'out', 'snapshot.json');
const reels = ['.', ...fs.readdirSync(__dirname).filter(d => /^release-v[\d.]+$/.test(d)).sort()]
  .filter(r => !only.length || only.includes(r));
const gc = global.gc || (() => {});
const md5 = b => crypto.createHash('md5').update(b).digest('hex');

(async () => {
  await loadAssets();
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');
  const result = {};
  for (const r of reels) {
    const dir = path.join(__dirname, r);
    const SC = require(path.join(dir, 'scenes'));
    const DUR = SC.DUR ?? require('./lib').DUR;
    const frames = {};
    for (let t = START; t < DUR; t += STEP) {
      SC.frame(ctx, t, t);
      frames[t.toFixed(2)] = md5(Buffer.from(ctx.getImageData(0, 0, W, H).data.buffer));
      gc();
      await new Promise(res => setImmediate(res));
    }
    execFileSync('node', [path.join(dir, 'audio.js')], { stdio: 'ignore' });
    const wav = md5(fs.readFileSync(path.join(dir, 'out', 'reel.wav')));
    result[r] = { frames, wav };
    process.stdout.write(`${r}: ${Object.keys(frames).length} frames\n`);
  }
  if (mode === 'save') {
    fs.mkdirSync(path.dirname(FILE), { recursive: true });
    const prev = fs.existsSync(FILE) ? JSON.parse(fs.readFileSync(FILE)) : {};
    fs.writeFileSync(FILE, JSON.stringify({ ...prev, ...result }, null, 1));
    console.log('saved', FILE);
    return;
  }
  const base = JSON.parse(fs.readFileSync(FILE));
  let bad = 0;
  for (const [r, v] of Object.entries(result)) {
    const b = base[r];
    if (!b) { console.log(`${r}: no baseline`); bad++; continue; }
    const diff = Object.keys({ ...b.frames, ...v.frames }).filter(k => b.frames[k] !== v.frames[k]);
    const wavOk = b.wav === v.wav;
    if (diff.length || !wavOk) { bad++; console.log(`${r}: DIFFERENT frames [${diff.join(', ')}]${wavOk ? '' : ' + audio'}`); }
    else console.log(`${r}: identical`);
  }
  process.exit(bad ? 1 : 0);
})();
