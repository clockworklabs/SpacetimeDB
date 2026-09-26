import fs from 'node:fs';
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {readArtifact,writeRunJson} from '/opt/stack-bench/dist/src/evidence/artifacts.js';
import {auditMutationWorkerRun} from '/opt/stack-bench/dist/src/references/reference-qualification-audit.js';
import {loadReferenceRegistry,selectReferenceFixture} from '/opt/stack-bench/dist/src/references/reference-fixtures.js';
const root=process.env.STACK_BENCH_RESULTS_DIR+'/diagnostics/role-revocation-20260926';
const hash=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const read=name=>readArtifact(root+'/'+name+'.json',{expectedKind:'reference_qualification'});
const parent=read('mongodb-remaining-final-2'),baseline=read('mongodb-remaining-reference');
const replacement=read('mongodb-password-control');
assert.equal(baseline.payload.ok,true);assert.equal(replacement.payload.ok,true);
assert.equal(replacement.payload.runs[0].mutations.caught,3);
assert.equal(replacement.payload.runs[0].mutations.total,3);
const expected=new Set(parent.payload.runs[0].workers.flatMap(w=>w.mutationIds));
assert.equal(expected.size,152);
const fixture=selectReferenceFixture(loadReferenceRegistry(),{backend:'mongodb',track:'ecommerce',level:3,recipe:'ecommerce.progression-catalog'});
const selected=new Map(),inputs=[],runs=[];
const replacements=new Set(['signin-skips-password-verification','password-verification-truncates-utf8','password-verification-rejects-every-login']);
for(const [name,isReplacement] of [...Array.from({length:6},(_,i)=>['mongodb-defects-'+i+'-2',false]),['mongodb-password-control',true]]) {
  const artifact=read(name),a=artifact.payload,run=a.runs[0];
  for(const key of ['engine','recipe','fixture','calibration','stackAdapter'])assert.deepEqual(artifact.identities[key],parent.identities[key]);
  assert.equal(a.fixtureSha256,baseline.payload.fixtureSha256);
  assert.equal(run.imageId,baseline.payload.runs[0].imageId);
  assert.equal(run.harnessSha256Before,baseline.payload.harnessSha256);
  assert.equal(run.harnessSha256After,baseline.payload.harnessSha256);
  const output=resolve(root,run.output),audit=auditMutationWorkerRun(output,fixture);
  if(name==='mongodb-defects-4-2')assert.deepEqual(audit.failures,[
    'run outcome is harness_failure','mutation control did not pass','signin-skips-password-verification is CAUGHT_OFF_ASSERTION']);
  else assert.equal(audit.ok,true,JSON.stringify(audit.failures));
  const controlPath=output+'/mutation-control.json',control=readArtifact(controlPath,{expectedKind:'mutation_control'}).payload;
  const used=[];
  for(const result of control.results) {
    if(!isReplacement&&replacements.has(result.id))continue;
    if(isReplacement)assert.equal(replacements.has(result.id),true);
    assert.equal(expected.has(result.id),true);assert.equal(selected.has(result.id),false);
    assert.equal(result.status,'CAUGHT');
    for(const key of ['targetMissing','targetHarnessFailures','targetInconclusive','targetSurvived','targetOffAssertion','setupFailures','missing'])assert.equal(result[key]?.length??0,0,result.id+': '+key);
    const report=control.gradeReports.find(g=>g.mutationId===result.id);
    assert.equal(report.status,'returned');assert.equal(hash(resolve(output,report.report.path)),report.report.sha256);
    selected.set(result.id,result);used.push(result.id);
  }
  inputs.push({path:name+'.json',sha256:hash(root+'/'+name+'.json'),controlPath,controlSha256:hash(controlPath),selectedMutations:used});
  runs.push(run);
}
assert.deepEqual([...selected.keys()].sort(),[...expected].sort());
const now=new Date().toISOString(),p=parent.payload,clean=baseline.payload.runs[0];
const combined={...clean,runId:parent.id+'-composed',durationMs:runs.reduce((sum,r)=>sum+r.durationMs,0),
  mutations:{caught:selected.size,total:selected.size},baselineOutput:clean.output,
  baselineHarnessSha256Before:clean.harnessSha256Before,baselineHarnessSha256After:clean.harnessSha256After};
const output=root+'/mongodb-remaining-composed.json';assert.equal(fs.existsSync(output),false);
writeRunJson(output,{...p,id:parent.id+'-composed',kind:'reference_qualification',identities:parent.identities,
  startedAt:now,completedAt:now,runs:[combined],ok:true,stable:true,sameImage:true,sameHarness:true,
  harnessSha256:baseline.payload.harnessSha256});
fs.copyFileSync(root+'/mongodb-remaining-final-2.json.inputs.json',output+'.inputs.json',fs.constants.COPYFILE_EXCL);
fs.writeFileSync(output+'.derivation.json',JSON.stringify({method:'Compose the verified 108-check clean baseline and 152 individually caught controls. The original failed group remains unchanged; all three password controls come from a fresh complete reference-plus-mutation run. No grade or source is edited.',
  parent:{path:'mongodb-remaining-final-2.json',sha256:hash(root+'/mongodb-remaining-final-2.json')},
  baseline:{path:'mongodb-remaining-reference.json',sha256:hash(root+'/mongodb-remaining-reference.json')},inputs,outputSha256:hash(output)},null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({output,checks:p.qualifiedCheckKeys.length,score:clean.score,mutations:selected.size}));
