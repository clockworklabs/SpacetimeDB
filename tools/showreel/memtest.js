// Guarded memory test: one worker, N frames; kills it if RSS exceeds a cap. REEL=dir tests another reel.
const { spawn } = require('child_process');
const fs = require('fs');
const [a, b, cap] = [+process.argv[2] || 1800, +process.argv[3] || 1890, +(process.argv[4] || 3000)];
const w = spawn(process.execPath, [...(process.env.NODE_FLAGS ? process.env.NODE_FLAGS.split(' ') : []), 'render.js', '--worker', a, b, 'out/memtest.mp4', '--samples', '5', ...(process.env.REEL ? ['--reel', process.env.REEL] : [])], { stdio: 'inherit' });
try { require('os').setPriority(w.pid, 19); } catch (e) {}
const rss = pid => { try { return +fs.readFileSync(`/proc/${pid}/status`, 'utf8').match(/VmRSS:\s+(\d+)/)[1] / 1024; } catch (e) { return 0; } };
let max = 0, samples = [];
const t0 = Date.now();
const iv = setInterval(() => {
  const m = rss(w.pid);
  max = Math.max(max, m);
  samples.push(Math.round(m));
  if (m > cap) { console.log(`KILLED at ${Math.round(m)} MB`); w.kill('SIGKILL'); }
}, 500);
w.on('exit', c => { clearInterval(iv); console.log(`exit ${c} in ${((Date.now() - t0) / 1000).toFixed(1)}s, frames ${b - a}, max RSS ${Math.round(max)} MB`); console.log('RSS trace (MB):', samples.filter((_, i) => i % 4 === 0).join(' ')); });
