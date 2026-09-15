import assert from 'node:assert/strict';
import { ADDRESS_BOOK_MIGRATION_RECIPE, applyAddressBookDefect } from '../../../dist/src/references/address-book-migration.js';
import { qualifyAddressBook, probeCrossAccountEdit } from './m8-live-address-book.mjs';
import { runAgent } from '../../../dist/commands/bench.js';
import { parseBenchArguments } from '../../../dist/commands/bench-arguments.js';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from '../../../dist/src/agents/agent-adapters.js';
import { createCheckEvidence } from '../../../dist/src/evidence/check-evidence.js';
import { repairEvidenceDecision } from '../../../dist/src/evidence/repair-evidence.js';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { createBackendLease, claimBackendResourcesWhenAvailable, resourceLockScope,
  backendResourceLockKeys, leaseFromEnv } from '../../../dist/src/runtime/backend-lease.js';
import { releaseBackendLease } from '../../../dist/src/runtime/backend-teardown.js';
import { activateAttemptBackend } from '../../../dist/src/stacks/hosted-lifecycle.js';
import { loadTrack, portsFor, dbName, moduleName } from '../../../dist/src/composition/tracks.js';
import { hashAppSource, snapshotAppSource } from '../../../dist/src/runtime/source-snapshot.js';
import { waitFor } from '../../../dist/src/stacks/lifecycle-readiness.js';
import { runBounded } from '../../../dist/src/runtime/bounded-process.js';
import { captureApplicationDiagnostics, controlAppServer, controlBackendRuntime } from '../../../dist/src/runtime/backend-control.js';
import { leasedSpacetimeTarget } from '../../../dist/src/runtime/spacetime-target.js';
import { getSpacetimeCheckoutState } from '../../../dist/src/stacks/backends/spacetime-operations.js';
import { getPostgresCheckoutState } from '../../../dist/src/stacks/backends/postgres-operations.js';
import { getMongoDbCheckoutState } from '../../../dist/src/stacks/backends/mongodb-operations.js';
import { assertLeasedContainer, requireLeasedDatabase } from '../../../dist/src/stacks/backend-reset-guard.js';
import { attemptDatabaseIdentity } from '../../../dist/src/stacks/hosted-database-identity.js';
import { canonicalizeDefinition } from '../../../dist/src/composition/definition-plan.js';
import { loadReferenceRegistry } from '../../../dist/src/references/reference-fixtures.js';
import { migrationCheckoutDifferences } from '../../../dist/src/stacks/migration-state.js';
import { capturePopulatedCheckpoint, restorePopulatedCheckpoint, restoreRepairSource, materializeAcceptedSource } from '../../../dist/src/runtime/source-materialization.js';
import { codingContainerAgentCommand, CODING_CONTAINER_SPACETIME_CLI,
  codingContainerAgentExecOptions } from '../../../dist/src/runtime/coding-container-policy.js';

const [backend, indexText, defect] = process.argv.slice(2), runIndex = Number(indexText);
assert(['postgres','mongodb','spacetime'].includes(backend) && Number.isSafeInteger(runIndex));
const checkpointRun = process.env.M8_CHECKPOINT_RUN === '1';
let migrationActive = false;
assert(!checkpointRun || !defect, 'checkpoint qualification requires the correct starting migration');
const track = loadTrack('ecommerce'), ports = portsFor(track, backend, runIndex);
const id = `m8-migration-${backend}-${Date.now()}`;
const directory = join(process.env.M8_OUTPUT_ROOT, id), app = join(directory,'source');
mkdirSync(app,{ recursive:true });
const leasePath = join(directory,'lease.json');
const lease = createBackendLease({ runId:id,backend,track:'ecommerce',runIndex,
  ...(backend === 'spacetime' ? { serverUri:`http://127.0.0.1:${18000+runIndex}`,
    module:moduleName(track,runIndex),dataDir:join(directory,'stdb-data') }
    : { database:dbName(track,runIndex) }) });
Object.assign(process.env,{ STACK_BENCH_LEASE:leasePath,STACK_BENCH_LEASE_TOKEN:lease.ownershipToken });
if (backend==='spacetime') process.env.STACK_BENCH_STDB_URI=lease.resources.serverUri;
const audit = { id,backend,runIndex,ports,startedAt:new Date().toISOString(),
  controllerImage:process.env.STACK_BENCH_CONTROLLER_IMAGE_ID,buildImage:process.env.STACK_BENCH_IMAGE,
  paidCalls:0,scope:'Live address-book reference migration diagnostic; no scored promotion' };
writeFileSync(join(directory,'driver.mjs'),readFileSync(process.argv[1]));
for(const name of ['m8-live-address-book.mjs','m8-native-request.mjs']) writeFileSync(join(directory,name),readFileSync(new URL(name,import.meta.url)));
const save = (name,value) => writeFileSync(join(directory,name),JSON.stringify(value,null,2)+'\n');
const spec = { backend,app,port:ports.vite,probe:'/' };
const agent=AGENT_ADAPTER_REGISTRY.get('reference-fixture');
assert.equal(agent.costLimit,'non-billable');
audit.agent=agentAdapterIdentity(agent);audit.sessions=[];
async function agentSession(mode,recipe,label) {
  const args={...parseBenchArguments(['node','bench','--backend',backend,'--track','ecommerce',
    '--levels','3','--run-index',String(runIndex),'--model','reference-fixture',
    '--agent-adapter','reference-fixture','--recipe',recipe]),recipeTasks:new Map(),recipeBindings:new Map()};
  const result=await runAgent(args,agent,mode,3,app);
  const session={label,recipe,...result};
  audit.sessions.push(session);save(`${label}.session.json`,session);
  assert(result.ok && result.costComplete && result.costUsd===0,`${label} did not complete as a non-billable session`);
  console.log(`${id} ${label}: passed`);
  return result;
}
const ownershipEvidence=observation=>({suites:{migration:{features:[{id:'address-book',criteria:[{
  id:'owner-write',points:1,evidence:createCheckEvidence({status:observation.outcome==='rejected'?'passed':'failed',
    code:'address_owner_write',phase:'assertion',observation,expected:'rejected',
    startedAtMs:observation.startedAtMs,completedAtMs:observation.completedAtMs})}]}]}}});
function checkpointWriter(start) {
  const {lease:active}=leaseFromEnv(process.env,{backend,active:true});
  const container=assertLeasedContainer(active.resources.buildContainer,execFileSync,30000,'checkpoint writer control');
  const record='/tmp/m8-checkpoint-writer.pid';
  if(start) {
    execFileSync('docker',['exec','-d',...codingContainerAgentExecOptions(),container,'sh','-c',
      `echo $$ > ${record}; exec sleep 600`],{timeout:30000});
    execFileSync('docker',['exec',container,'sh','-c',
      `i=0; until test -s ${record}; do i=$((i+1)); test "$i" -lt 50 || exit 1; sleep 0.1; done`],{timeout:30000});
  } else execFileSync('docker',['exec',container,'sh','-c',
    `pid=$(cat ${record}); test ! -d /proc/"$pid"`],{timeout:30000});
}
async function run(name,argv,binary=process.execPath) {
  const result = await runBounded(binary,argv,{timeoutMs:1200000,stdio:'ignore',
    logs:{stdout:join(directory,`${name}.stdout.log`),stderr:join(directory,`${name}.stderr.log`)} });
  audit[name]={ok:result.ok,code:result.code,timedOut:result.timedOut,error:result.error?.message};
  console.log(`${id} ${name}: ${result.ok?'passed':'failed'}`);
  assert(result.ok,`${name} failed; see retained logs`);
}
async function grade(name,steps,actors=['a','b','admin','staff']) {
  const scenario = {schemaVersion:1,track:'ecommerce',level:3,name:`M8 ${name} diagnostic`,
    features:[{id:9800,name,actors,setup:[],criteria:[{id:'9800a',category:'production',
      points:0,desc:name,steps}]}]};
  save(`${name}.scenario.json`,scenario);
  await run(name,['dist/grader/grade.js','--backend',backend,'--app',app,'--url',`http://127.0.0.1:${ports.vite}`,
    '--level','3','--spec',join(directory,`${name}.scenario.json`),'--out',join(directory,`${name}.grade.json`)]);
  const result=JSON.parse(readFileSync(join(directory,`${name}.grade.json`),'utf8'));
  const checks=result.payload.features.flatMap(feature=>feature.criteria);
  assert(checks.length===1 && checks.every(check=>check.evidence.status==='passed'),`${name} observation failed`);
}
const login=(actor,name,password)=>({do:'signIn',actor,name,password,exact:true});
const fill=(actor,testid,text,scope)=>({do:'fill',actor,testid,text,...(scope?{in:scope}:{})});
const click=(actor,testid,scope)=>({do:'click',actor,testid,...(scope?{in:scope}:{})});
const expect=(actor,testid,contains)=>({do:'expect',actor,testid,contains,within:10000});
const checkout={do:'callConcurrently',actors:['a'],action:'checkout',
  namedAction:{id:'checkout',path:'/api/checkout',reducer:'checkout',args:[]},requests:1,requestTimeoutMs:30000,settleMs:1500};
function seedSteps() {
  const steps=[];
  for(const [actor,name] of [['a','m8-customer'],['b','admin-helper']]) {
    steps.push({do:'signUp',actor,name,exact:true,password:'m8-baseline-password'});
    for(const item of ['Keyboard',actor==='a'?'Headphones':'Laptop Stand']) steps.push({...fill(actor,'search-input',item),enter:true},
      expect(actor,'item-card',item),click(actor,'add-to-cart',{testid:'item-card',contains:item}));
    steps.push({...checkout,actors:[actor]},{do:'expectCallOutcomes'});
    steps.push(click(actor,'profile-link'),fill(actor,'profile-name',name),
      fill(actor,'profile-address',actor==='a'?'12 Café Street, Apt 2':'34 Oak Street, Floor 3'),
      click(actor,'profile-save'),expect(actor,'profile-address-summary',actor==='a'?'12 Café Street':'34 Oak Street'));
  }
  steps.push(login('staff','staff','stackbench-staff-2026'),click('staff','staff-link'),
    expect('staff','queue-item','Headphones'),click('staff','ship-submit',{testid:'queue-item',contains:'Headphones'}),
    {do:'expect',actor:'staff',testid:'fulfilment-panel',attribute:'data-submit-state',value:'succeeded',within:10000},
    login('admin','admin','stackbench-admin-2026'),click('admin','admin-link'),
    fill('admin','price-input','12.34',{testid:'admin-item-row',contains:'Keyboard'}),
    click('admin','price-submit',{testid:'admin-item-row',contains:'Keyboard'}));
  for(const name of ['M8 Café / blue','M8 Cafe - blue']) steps.push(fill('admin','catalog-name',name),
    fill('admin','catalog-category','Accessories'),fill('admin','catalog-price','3.17'),
    fill('admin','catalog-variants',''),click('admin','catalog-save'));
  steps.push({do:'reload',actor:'a',settleMs:1500},{...fill('a','search-input','M8'),enter:true},
    expect('a','item-card','M8 Café / blue'),expect('a','item-card','M8 Cafe - blue'));
  return steps;
}
function checkoutState(account) {
  const input={account,item:'Keyboard',app};
  const active=leaseFromEnv(process.env,{backend,active:true}).lease;
  if(backend==='spacetime') return getSpacetimeCheckoutState({...input,addressBookMigration:migrationActive,spacetime:leasedSpacetimeTarget({requireBuildContainer:true})});
  return (backend==='postgres'?getPostgresCheckoutState:getMongoDbCheckoutState)({...input,lease:requireLeasedDatabase(active)});
}
function nativeTables(tables) {
  const target=leasedSpacetimeTarget({requireBuildContainer:true});
  const container=assertLeasedContainer(target.buildContainer,execFileSync,60000,'M8 stored read');
  const raw=JSON.parse(execFileSync('docker',['exec',...codingContainerAgentExecOptions(),container,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,['subscribe',target.mod,'-s',target.containerUri,
      '--print-initial-update','--num-updates','0','--timeout','30',...tables.map(t=>`SELECT * FROM ${t}`)])],{encoding:'utf8',timeout:60000}));
  assert(raw && !Array.isArray(raw) && typeof raw==='object');
  assert(Object.keys(raw).every(t=>tables.includes(t)));
  return Object.fromEntries(tables.map(t=>{
    if(!(t in raw)) return [t,[]]; // The CLI omits empty tables from initial updates.
    assert(Array.isArray(raw[t].inserts)&&Array.isArray(raw[t].deletes)&&!raw[t].deletes.length);
    return [t,raw[t].inserts];
  }));
}
function storedBooks() {
  const active=leaseFromEnv(process.env,{backend,active:true}).lease;
  if(backend==='spacetime') {
    const raw=nativeTables(['account','customer_profile','address_entry']);
    const rows=t=>raw[t];
    return rows('account').filter(a=>['m8-customer','admin-helper'].includes(a.username)).map(a=>{
      const profile=rows('customer_profile').find(p=>String(p.account_id)===String(a.id));
      return {accountId:String(a.id),legacyProfile:{name:profile?.name??'',address:profile?.address??''},
        entries:rows('address_entry').filter(e=>String(e.account_id)===String(a.id)).sort((a,b)=>Number(a.id)-Number(b.id))
          .map(e=>({id:String(e.id),name:e.name,address:e.address,isDefault:e.is_default}))};
    });
  }
  const database=requireLeasedDatabase(active);
  const container=assertLeasedContainer(database.resources.container,execFileSync,60000,'address-book stored read');
  if(backend==='mongodb') {
    const {user,password}=attemptDatabaseIdentity(active.ownershipToken);
    const script=`const s=db.getMongo().startSession();try{s.startTransaction({readConcern:{level:'snapshot'}});const d=s.getDatabase(db.getName());
      const result=d.users.find({username:{$in:['m8-customer','admin-helper']}}).toArray().map(a=>{const p=d.progressionprofiles.findOne({userId:a._id});
        const book=d.addressbooks.findOne({_id:a._id});if(!book)throw Error('stored address book missing');
        return {accountId:String(a._id),legacyProfile:{name:p?.name??'',address:p?.address??''},
        entries:book.entries.map(e=>({id:String(e._id),name:e.name,address:e.address,isDefault:e.isDefault}))};});
      s.commitTransaction();print(JSON.stringify(result));}finally{s.endSession();}`;
    return JSON.parse(execFileSync('docker',['exec',container,'mongosh',database.resources.database,
      '--username',user,'--password',password,'--authenticationDatabase',database.resources.database,'--quiet','--eval',script],{encoding:'utf8',timeout:60000}));
  }
  const sql=`SELECT COALESCE(json_agg(book),'[]'::json)::text FROM (
    SELECT a.id::text AS "accountId", json_build_object('name',a.profile_name,'address',a.profile_address) AS "legacyProfile",
    COALESCE((SELECT json_agg(json_build_object('id',e.id::text,'name',e.name,'address',e.address,'isDefault',e.is_default) ORDER BY e.id)
      FROM address_entry e WHERE e.account_id=a.id),'[]'::json) AS entries
    FROM account a WHERE a.username IN ('m8-customer','admin-helper') ORDER BY a.id
  ) book;`;
  return JSON.parse(execFileSync('docker',['exec','-i',container,'psql','-U','appuser','-d',database.resources.database,'-v','ON_ERROR_STOP=1','-At'],
    {encoding:'utf8',input:sql,timeout:60000}));
}
function snapshot() {
  // The registered source is fixed. These explicit table lists are not schema discovery.
  const active=leaseFromEnv(process.env,{backend,active:true}).lease;
  let state;
  if(backend==='spacetime') {
    const tables=['account','customer_profile','item','warehouse','stock','cart_item','reservation','customer_order','order_item','payment_record','order_item_stock'];
    state=nativeTables(tables);
  } else {
    const database=requireLeasedDatabase(active),container=assertLeasedContainer(database.resources.container,execFileSync,60000,'M8 baseline read');
    if(backend==='postgres') {
      const tables=['account','item','warehouse','stock','cart','cart_item','cart_reservation_allocation','orders','order_item'];
      const sql=`SELECT json_build_object(${tables.map(t=>`'${t}',COALESCE((SELECT json_agg(row_to_json(r)) FROM ${t} r),'[]'::json)`).join(',')})::text;`;
      state=JSON.parse(execFileSync('docker',['exec','-i',container,'psql','-U','appuser','-d',database.resources.database,'-v','ON_ERROR_STOP=1','-At'],
        {encoding:'utf8',input:sql,timeout:60000}));
    } else {
      const tables=['users','progressionprofiles','item','warehouse','stock','carts','orders','progressionpayments'];
      const {user,password}=attemptDatabaseIdentity(active.ownershipToken);
      const script=`const session=db.getMongo().startSession();try{session.startTransaction({readConcern:{level:'snapshot'}});const store=session.getDatabase(db.getName());const result=Object.fromEntries(${JSON.stringify(tables)}.map(t=>[t,store.getCollection(t).find({}).toArray()]));session.commitTransaction();print(EJSON.stringify(result));}finally{session.endSession();}`;
      state=JSON.parse(execFileSync('docker',['exec',container,'mongosh',database.resources.database,
        ...(database.resources.network?['--username',user,'--password',password,'--authenticationDatabase',database.resources.database]:[]),
        '--quiet','--eval',script],{encoding:'utf8',timeout:60000}));
    }
  }
  state=Object.fromEntries(Object.entries(state).map(([table,rows])=>{
    assert(Array.isArray(rows),`${table} did not return rows`);
    return [table,rows.map(canonicalizeDefinition).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)))];
  }));
  return {state,sha256:createHash('sha256').update(JSON.stringify(canonicalizeDefinition(state))).digest('hex')};
}
try {
  await claimBackendResourcesWhenAvailable(leasePath,lease,{...resourceLockScope(),keys:backendResourceLockKeys(lease,ports,[`workspace:${app}`])});
  activateAttemptBackend({leasePath,lease,ports});
  await agentSession('build','ecommerce.progression-catalog','deploy');
  audit.source=hashAppSource(app);
  const reference=loadReferenceRegistry().fixtures.find(f=>f.backend===backend&&f.track==='ecommerce');
  assert.equal(audit.source.sha256,reference.imported.sourceSha256);
  await grade('populate',seedSteps());
  // The cumulative reference schedules delivery after shipping. Let that real
  // operation finish before freezing the migration baseline; do not hide status changes.
  audit.phase='settle-existing-delivery';
  const deadline=Date.now()+120000;
  while(!checkoutState('m8-customer').state.orders.some(order=>order.status==='delivered')) {
    if(Date.now()>deadline) throw new Error('Existing delivery did not settle before baseline capture');
    await delay(2000);
  }
  const checks=['m8-customer','admin-helper'].map(checkoutState);
  save('before-checkout.json',checks);
  for(const {state} of checks) {
    assert.equal(state.orders.length,2);assert.equal(state.payments.length,2);
    assert.deepEqual(state.orders.map(o=>o.status).sort(),['delivered','pending']);
    assert(state.orders.every(o=>o.lines.length===2),'each order needs two preserved lines');
    assert.equal(state.priceMinor,1234);
    assert(state.orders.every(o=>o.lines.some(line=>line.itemId===state.itemId&&line.priceMinor!==1234)));
    assert.equal(state.cart.length,0);assert.equal(state.reservations.length,0);
  }
  const before=snapshot();save('before-state.json',before);
  const accounts=before.state[backend==='mongodb'?'users':'account'];
  for(const [name,admin,staff] of [['admin',true,false],['staff',false,true],['m8-customer',false,false],['admin-helper',false,false]]) {
    const matches=accounts.filter(row=>row.username===name);
    assert.equal(matches.length,1,`missing or duplicate account ${name}`);
    assert.equal(matches[0][backend==='mongodb'?'isAdmin':'is_admin'],admin);
    assert.equal(matches[0][backend==='mongodb'?'isStaff':'is_staff'],staff);
  }
  const addresses=backend==='postgres'?accounts.filter(row=>row.profile_address).map(row=>row.profile_address)
    :before.state[backend==='mongodb'?'progressionprofiles':'customer_profile'].map(row=>row.address);
  assert.deepEqual(addresses.sort(),['12 Café Street, Apt 2','34 Oak Street, Floor 3'].sort());
  audit.population={orders:2,payments:2,orderLines:4,customers:2,scopeTables:Object.keys(before.state),
    rows:Object.fromEntries(Object.entries(before.state).map(([table,rows])=>[table,rows.length]))};
  audit.beforeCheckout=checks.map(value=>value.state);
  if(checkpointRun) {
    audit.phase='capture-populated-start';
    checkpointWriter(true);
    audit.initialCheckpoint=await capturePopulatedCheckpoint(join(directory,'initial-checkpoint'),spec);
    checkpointWriter(false);audit.detachedWriterStopped=true;
    await assert.rejects(capturePopulatedCheckpoint(join(directory,'wrong-workspace-checkpoint'),
      {...spec,app:join(directory,'initial-checkpoint','source')}),/does not match its leased build workspace/);
    assert(!existsSync(join(directory,'wrong-workspace-checkpoint')));
    audit.wrongWorkspaceRejected=true;
    assert.deepEqual(snapshot().state,before.state,'checkpoint capture changed populated starting data');
  }
  if(defect) audit.defect=defect;
  await agentSession('upgrade',ADDRESS_BOOK_MIGRATION_RECIPE+(defect?`.${defect}`:''),'migration-upgrade');
  audit.migratedSource=hashAppSource(app);
  snapshotAppSource(app,join(directory,'first-submission'));
  audit.firstSubmission={directory:'first-submission',sha256:audit.migratedSource.sha256};
  migrationActive=true;
  if(backend==='spacetime') {
    await controlBackendRuntime(spec,'restart');
    await controlAppServer(spec,'restart');
    audit.restartBoundary='leased SpacetimeDB process, then application startup';
  } else {
    audit.restartBoundary='application startup against retained live database; no database crash claim';
  }
  if(backend!=='spacetime' && !defect) {
    const active=leaseFromEnv(process.env,{backend,active:true}).lease;
    const container=assertLeasedContainer(active.resources.buildContainer,execFileSync,60000,'M8 source typecheck');
    await run('server-typecheck',['exec',...codingContainerAgentExecOptions(),'-w','/app/server',container,
      ...codingContainerAgentCommand('node',['node_modules/typescript/bin/tsc','--noEmit'])],'docker');
  }
  const after=snapshot();save('after-state.json',after);
  audit.phase='preservation-after-upgrade';
  assert.deepEqual(after.state,before.state,'populated business records changed across restart');
  audit.beforeSha256=before.sha256;audit.afterSha256=after.sha256;
  save('after-checkout.json',['m8-customer','admin-helper'].map(checkoutState));
  await qualifyAddressBook({audit,before,snapshot,checkoutState,storedBooks,save,url:'http://127.0.0.1:'+ports.vite,restart:async()=>{
    if(backend==='spacetime') await controlBackendRuntime(spec,'restart');
    await controlAppServer(spec,'restart');
  }});
  audit.sourceAfter=hashAppSource(app);
  assert.equal(audit.sourceAfter.sha256,audit.migratedSource.sha256);
  if(checkpointRun) {
    audit.phase='capture-accepted-migration';
    const accepted=snapshot(),books=storedBooks();
    const owner=books.find(book=>book.accountId===audit.beforeCheckout[0].accountId);
    assert(owner?.entries.length,'ownership repair control needs a saved owner entry');
    const probe=()=>probeCrossAccountEdit({backend,url:`http://127.0.0.1:${ports.vite}`,id:owner.entries[0].id});
    const baselineOwnership=await probe();
    assert.equal(baselineOwnership.outcome,'rejected');
    assert.deepEqual(storedBooks(),books,'correct ownership control changed stored data');
    const baselineEvidence=ownershipEvidence(baselineOwnership);
    save('accepted-ownership.json',baselineEvidence);
    const changePrice=async name=>{
      await grade(name,[login('admin','admin','stackbench-admin-2026'),click('admin','admin-link'),
        fill('admin','price-input','8.76',{testid:'admin-item-row',contains:'Keyboard'}),
        click('admin','price-submit',{testid:'admin-item-row',contains:'Keyboard'})],['admin']);
      await waitFor(async()=>checkoutState('m8-customer').state.priceMinor===876,10000,'the candidate price write');
      assert.notDeepEqual(snapshot().state,accepted.state,'repair control did not change data');
    };
    save('accepted-state.json',accepted);save('accepted-books.json',books);
    audit.acceptedCheckpoint=await capturePopulatedCheckpoint(join(directory,'accepted-checkpoint'),spec);
    await assert.rejects(restoreRepairSource(join(directory,'initial-checkpoint','source'),app,spec,
      undefined,undefined,join(directory,'accepted-checkpoint')),/repair source does not match/);
    assert.deepEqual(snapshot().state,accepted.state,'invalid checkpoint request changed the live database');
    audit.mismatchedSourceRejected=true;
    // Qualify the failure we are replacing: the fresh-build rollback really
    // recreates an empty grading database and loses this populated task's data.
    audit.phase='empty-reset-control';
    await restoreRepairSource(join(directory,'accepted-checkpoint','source'),app,spec);
    assert.notDeepEqual(snapshot().state,accepted.state,'empty-reset control did not lose populated data');
    audit.emptyResetDetected=true;
    await restoreRepairSource(join(directory,'accepted-checkpoint','source'),app,spec,
      undefined,undefined,join(directory,'accepted-checkpoint'));
    assert.deepEqual(snapshot().state,accepted.state,'accepted records did not survive recovery from empty reset');
    for(let round=1;round<=2;round++) {
      audit.phase=`rejected-repair-${round}`;
      await changePrice(`change-price-${round}`);
      await agentSession('fix',`${ADDRESS_BOOK_MIGRATION_RECIPE}.cross-account`,`rejected-repair-${round}`);
      assert.notEqual(hashAppSource(app).sha256,audit.migratedSource.sha256);
      snapshotAppSource(app,join(directory,`rejected-repair-${round}-source`));
      const failedOwnership=await probe();
      assert.equal(failedOwnership.outcome,'committed','defective control did not allow an unauthorized edit');
      assert.notDeepEqual(storedBooks(),books,'defective control did not change the owner record');
      const failedEvidence=ownershipEvidence(failedOwnership);
      const decision=repairEvidenceDecision(baselineEvidence,failedEvidence);
      save(`rejected-repair-${round}.evidence.json`,{evidence:failedEvidence,decision});
      save(`rejected-repair-${round}.state.json`,{business:snapshot(),books:storedBooks()});
      assert.equal(decision.action,'rollback-regression');
      await restoreRepairSource(join(directory,'accepted-checkpoint','source'),app,spec,
        undefined,undefined,join(directory,'accepted-checkpoint'));
      assert.deepEqual(snapshot().state,accepted.state,'accepted business data did not restore');
      assert.deepEqual(storedBooks(),books,'accepted address book did not restore');
      assert.equal(hashAppSource(app).sha256,audit.migratedSource.sha256);
      audit[`acceptedRestore${round}`]=true;
    }
    await agentSession('fix',ADDRESS_BOOK_MIGRATION_RECIPE,'accepted-repair');
    assert.equal(hashAppSource(app).sha256,audit.migratedSource.sha256);
    const repairedEvidence=ownershipEvidence(await probe());
    const keep=repairEvidenceDecision(baselineEvidence,repairedEvidence);
    assert.equal(keep.action,'keep');
    assert.deepEqual(snapshot().state,accepted.state);assert.deepEqual(storedBooks(),books);
    save('accepted-repair.evidence.json',{evidence:repairedEvidence,decision:keep});
    await changePrice('state-only-repair');
    await agentSession('fix',ADDRESS_BOOK_MIGRATION_RECIPE,'unchanged-source-repair');
    assert.equal(hashAppSource(app).sha256,audit.migratedSource.sha256);
    assert.notDeepEqual(snapshot().state,accepted.state,'unchanged-source control did not retain the data change');
    await restoreRepairSource(join(directory,'accepted-checkpoint','source'),app,spec,
      undefined,undefined,join(directory,'accepted-checkpoint'));
    assert.deepEqual(snapshot().state,accepted.state);assert.deepEqual(storedBooks(),books);
    audit.unchangedSourceDataRestored=true;
    audit.phase='restore-pre-migration';
    await restorePopulatedCheckpoint(join(directory,'initial-checkpoint'),spec);
    assert.equal(hashAppSource(app).sha256,audit.source.sha256);
    migrationActive=false;
    assert.deepEqual(snapshot().state,before.state,'original populated records did not restore');
    assert.deepEqual(migrationCheckoutDifferences(audit.beforeCheckout,
      ['m8-customer','admin-helper'].map(checkoutState).map(value=>value.state)),[]);
    await grade('purchase-after-restore',[login('a','m8-customer','m8-baseline-password'),
      {do:'dbRecordCheckout',account:'m8-customer',item:'Keyboard',as:'before-restore-purchase'},
      {...fill('a','search-input','Keyboard'),enter:true},expect('a','item-card','Keyboard'),
      click('a','add-to-cart',{testid:'item-card',contains:'Keyboard'}),
      {do:'expectNumber',actor:'a',testid:'cart-count',equals:1,within:10000},
      {do:'dbRecordCheckout',account:'m8-customer',item:'Keyboard',as:'prepared-restore-purchase'},
      checkout,{do:'expectCallOutcomes'},
      {do:'dbExpectCheckout',before:'before-restore-purchase',prepared:'prepared-restore-purchase',quantity:1}],['a']);
    await restorePopulatedCheckpoint(join(directory,'initial-checkpoint'),spec);
    assert.deepEqual(snapshot().state,before.state,'second original restore retained later purchases');
    audit.initialRestores=2;audit.purchaseAfterRestore=true;

    // A repair checkpoint proves rollback, not that saved code can perform the
    // migration. Reapply candidate source to the immutable original database.
    const verifyImport=async label=>{
      const observed={...audit};
      try {
        await qualifyAddressBook({audit:observed,before,snapshot,checkoutState,storedBooks,
          save:(name,value)=>save(`${label}-${name}`,value),url:`http://127.0.0.1:${ports.vite}`,importOnly:true});
      } finally {save(`${label}.json`,{source:hashAppSource(app).sha256,phase:observed.phase});}
    };
    audit.phase='saved-source-replay';
    await materializeAcceptedSource(join(directory,'first-submission'),app,spec);
    migrationActive=true;
    await verifyImport('saved-source-replay');
    audit.savedSourceReplayPassed=true;

    const broken=join(directory,'no-import-candidate');
    snapshotAppSource(app,broken);
    audit.noImportSource=applyAddressBookDefect(broken,backend,'no-import').sha256;
    await materializeAcceptedSource(broken,app,spec);
    await verifyImport('no-import-existing-state');
    audit.noImportExistingStatePassed=true;

    await restorePopulatedCheckpoint(join(directory,'initial-checkpoint'),spec);
    await materializeAcceptedSource(broken,app,spec);
    let replayFailure;
    try {await verifyImport('no-import-original-state');}
    catch(error) {replayFailure={code:error.code,message:error.message};}
    save('no-import-original-state-failure.json',replayFailure??null);
    assert.equal(replayFailure?.code,'ERR_ASSERTION');
    assert.match(replayFailure.message,/imported address count differs/);
    audit.noImportReplayRejected=true;
    await restorePopulatedCheckpoint(join(directory,'initial-checkpoint'),spec);
    migrationActive=false;
    assert.deepEqual(snapshot().state,before.state,'source replay control changed the original state after restoration');
  }
  audit.preserved=true;
} catch(error) {
  audit.error=error.message;audit.failureKind=error.code==='ERR_ASSERTION'?'observation-mismatch':'execution-error';
  try { audit.applicationDiagnostics=captureApplicationDiagnostics(join(directory,'application-error.log')); }
  catch(diagnosticError) { audit.diagnosticError=diagnosticError.message; }
  process.exitCode=1;console.error(error.message);
}
finally {
  try {audit.released=existsSync(leasePath)?releaseBackendLease(leasePath,lease.ownershipToken):true;}
  catch(error) {audit.cleanupError=error.message;audit.released=false;}
  if(!audit.released)process.exitCode=1;
  audit.finishedAt=new Date().toISOString();save('audit.json',audit);
  console.log(JSON.stringify({directory,preserved:audit.preserved,error:audit.error,released:audit.released}));
}
