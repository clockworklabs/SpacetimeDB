// Model-free diagnostic. Uses frozen Vite bytes; no production files are changed.
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';
const require = createRequire('/opt/stack-bench/package.json');
const {chromium} = require('playwright');
const {installResponseLoss} = await import('/opt/stack-bench/dist/grader/response-loss.js');
const viteRoot = '/var/lib/docker/volumes/stack-bench-state/_data/work/reference-live-spacetime-iB9ohP/app/client/node_modules/vite';
const {createServer} = await import(viteRoot + '/dist/node/index.js');
const sha = b => createHash('sha256').update(b).digest('hex');
const result = {viteVersion:JSON.parse(fs.readFileSync(viteRoot+'/package.json')).version,
  viteClientSha256:sha(fs.readFileSync(viteRoot+'/dist/client/client.mjs')),
  gateSha256:sha(fs.readFileSync('/opt/stack-bench/dist/grader/response-loss.js')),
  probeSha256:sha(fs.readFileSync('/evidence/response-loss-real-vite-probe.mjs')),
  rows:[],cleanup:{browserClosed:false,viteClosed:false,socketsClosed:false},ok:false};
const root = fs.mkdtempSync('/tmp/response-loss-vite-');
fs.writeFileSync(root+'/index.html',`<!doctype html><p>ready</p><script type="module">
window.documentId=crypto.randomUUID();window.received=[];
window.app=new WebSocket('ws://'+location.host+'/?token=application-session','application.test');
app.onmessage=e=>received.push(e.data);</script>`);
let vite, browser;
const sockets = new Set();
try {
  vite = await createServer({root,configFile:false,cacheDir:root+'/cache',logLevel:'silent',
    server:{host:'127.0.0.1',port:0,watch:null},optimizeDeps:{noDiscovery:true,include:[]}});
  vite.httpServer.on('upgrade',(req,socket) => {
    if(req.headers['sec-websocket-protocol'] !== 'application.test') return;
    sockets.add(socket); socket.on('close',()=>sockets.delete(socket)); socket.on('error',()=>{});
    const accept=createHash('sha1').update(req.headers['sec-websocket-key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+'\r\nSec-WebSocket-Protocol: application.test\r\n\r\n');
    let pending=Buffer.alloc(0);
    socket.on('data',chunk=>{
      pending=Buffer.concat([pending,chunk]);
      while(pending.length>=2){
        const op=pending[0]&15,n=pending[1]&127;
        assert(n<126);assert(pending[1]&128);
        if(pending.length<6+n)return;
        const data=Buffer.from(pending.subarray(6,6+n));
        for(let i=0;i<n;i++)data[i]^=pending[2+i%4];
        pending=pending.subarray(6+n);
        if(op===8){socket.end();return;}
        assert.equal(op,1);
        socket.write(Buffer.concat([Buffer.from([0x81,data.length]),data]));
      }
    });
  });
  await vite.listen();
  const address=vite.httpServer.address(),url='http://127.0.0.1:'+address.port;
  browser=await chromium.launch({headless:true,executablePath:chromium.executablePath(),args:['--no-sandbox']});
  result.browserVersion=browser.version();
  for(const fault of [false,true]){
    const context=await browser.newContext(),gate=fault?await installResponseLoss(context):null;
    const page=await context.newPage(),navigation=[],consoleMessages=[],websockets=[];
    page.on('framenavigated',f=>{if(f===page.mainFrame())navigation.push({at:Date.now(),path:new URL(f.url()).pathname});});
    page.on('console',m=>{if(consoleMessages.length<40)consoleMessages.push(m.text());});
    page.on('websocket',ws=>websockets.push({path:new URL(ws.url()).pathname,tokenKind:new URL(ws.url()).searchParams.get('token')==='application-session'?'application':'vite',closed:false,
      ...(ws.on('close',()=>{const found=websockets.findLast(s=>!s.closed&&s.tokenKind===(new URL(ws.url()).searchParams.get('token')==='application-session'?'application':'vite'));if(found)found.closed=true;}),{})}));
    try {
      await page.goto(url);
      await page.waitForFunction(()=>window.app?.readyState===WebSocket.OPEN);
      for(let n=0;n<200&&!consoleMessages.some(s=>s.includes('[vite] connected'));n++)await delay(20);
      assert(consoleMessages.some(s=>s.includes('[vite] connected')),'Actual Vite client must connect');
      const served=await(await fetch(url+'/@vite/client')).text();
      const before=await page.evaluate(()=>window.documentId),navBefore=navigation.length;
      if(gate)gate.arm();
      await page.evaluate(()=>app.send('business-write'));
      for(let n=0;n<200;n++){
        if(gate?gate.evidence().events.some(e=>e.kind==='ws-drop'):(await page.evaluate(()=>received.length))===1)break;
        await delay(10);
      }
      if(gate)assert(gate.evidence().events.some(e=>e.kind==='ws-drop'&&e.path==='/'));
      else assert.deepEqual(await page.evaluate(()=>received),['business-write']);
      const finishedAt=Date.now();
      await gate?.finish();
      await delay(2500);
      const after=await page.evaluate(()=>window.documentId);
      const row={fault,servedClientSha256:sha(served),finishedAt,navigationsBefore:navBefore,
        navigationsAfter:navigation.length,documentChanged:before!==after,navigation,consoleMessages,websockets,
        faultEvidence:gate?.evidence()??null,explicitReloads:0};
      result.rows.push(row);
      assert.equal(row.documentChanged,fault,'Only cutting the real Vite socket should cause an unsolicited document reload');
      assert.equal(navigation.length-navBefore,fault?1:0);
      if(gate){assert.deepEqual(gate.evidence().errors,[]);assert.equal(gate.evidence().truncated,false);}
    }finally{await gate?.finish();await context.close();}
  }
  result.ok=true;
}catch(error){result.error={name:error.name,message:error.message};process.exitCode=1;}
finally{
  if(browser){await browser.close();result.cleanup.browserClosed=true;}
  for(const socket of sockets)socket.destroy();
  if(vite){await vite.close();result.cleanup.viteClosed=true;}
  result.cleanup.socketsClosed=sockets.size===0;
  fs.writeFileSync('/evidence/response-loss-real-vite-probe.json',JSON.stringify(result,null,2)+'\n');
  console.log(JSON.stringify({ok:result.ok,version:result.viteVersion,rows:result.rows.map(r=>({fault:r.fault,documentChanged:r.documentChanged,navigations:r.navigationsAfter-r.navigationsBefore})),cleanup:result.cleanup,error:result.error}));
}
