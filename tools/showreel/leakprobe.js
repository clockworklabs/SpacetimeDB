const { W, H, loadAssets, createCanvas } = require('./lib');
const { frame } = require('./scenes');
const mode = process.argv[2] || 'frame';
const rss = () => Math.round(process.memoryUsage().rss / 1048576);
(async () => {
  await loadAssets();
  const c = createCanvas(W, H), ctx = c.getContext('2d');
  const trace = [];
  for (let i = 0; i < 300; i++) {
    const t = 30 + i / 60;
    if (mode === 'frame' || mode === 'both') frame(ctx, t);
    if (mode === 'read' || mode === 'both') ctx.getImageData(0, 0, W, H);
    if (global.gc) global.gc();
    if (i % 30 === 0) trace.push(rss());
    if (rss() > 3000) { console.log('ABORT >3GB'); break; }
  }
  console.log(mode, 'RSS trace (MB):', trace.join(' '), 'final', rss());
})();
