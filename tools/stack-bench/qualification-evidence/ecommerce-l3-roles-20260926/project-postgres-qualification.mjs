import fs from 'node:fs';
import assert from 'node:assert/strict';
import {dirname,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {readArtifact,writeRunJson} from '/opt/stack-bench/dist/src/evidence/artifacts.js';
import {referenceRunFromMutationBaseline} from '/opt/stack-bench/dist/src/references/reference-live.js';
import {auditMutationWorkerRun} from '/opt/stack-bench/dist/src/references/reference-qualification-audit.js';
import {loadReferenceRegistry,selectReferenceFixture} from '/opt/stack-bench/dist/src/references/reference-fixtures.js';
import {qualificationScopeIdentity} from '/opt/stack-bench/dist/src/composition/qualification-scope.js';
import {calibrationQualificationRelease,mutationExecutionSha256,hasExactSelectedPackRuntime} from '/opt/stack-bench/dist/src/composition/calibration-compiler.js';
import {validateQualificationDocuments} from '/opt/stack-bench/dist/src/composition/qualification-slices.js';
import {mutationForRecipe,mutationTargetKeys} from '/opt/stack-bench/dist/src/evidence/mutation-analysis.js';
import {measureGradePackRuntime,aggregatePackRuntime} from '/opt/stack-bench/dist/src/composition/pack-runtime.js';
const root=process.env.STACK_BENCH_RESULTS_DIR+'/diagnostics/role-revocation-20260926';
const hash=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const parentPath=root+'/postgres-remaining-final-2.json',parent=readArtifact(parentPath),p=parent.payload;
const snapshot=JSON.parse(fs.readFileSync(parentPath+'.inputs.json'));
const full=validateQualificationDocuments(snapshot.documents).release;
const fixture=selectReferenceFixture(loadReferenceRegistry(),{backend:'postgres',track:'ecommerce',level:3,recipe:'ecommerce.progression-catalog'});
const clean=referenceRunFromMutationBaseline(root,p.runs[0],fixture,{release:full,level:3,selectedCheckKeys:p.qualifiedCheckKeys});
assert.equal(clean.ok,true,JSON.stringify(clean.failures));assert.equal(clean.score,'169/169');
const excluded=p.qualifiedCheckKeys.filter(k=>k.includes('customer-profile'));
assert.equal(excluded.length,3);
const keys=p.qualifiedCheckKeys.filter(k=>!excluded.includes(k)),selected=new Set(keys);
assert.equal(keys.length,105);
const release=calibrationQualificationRelease({qualification:{checks:keys}},full,[]).release;
const baselinePath=resolve(root,clean.output,'grading/bundle.json'),bundle=readArtifact(baselinePath).payload;
const criteria=[],reports=[];
for(const [suiteId,suite] of Object.entries(bundle.suites)) {
  if(suiteId==='lint')continue;
  const features=suite.features.map(f=>({...f,criteria:f.criteria.filter(c=>selected.has(c.stableKey))})).filter(f=>f.criteria.length);
  if(!features.length)continue;
  for(const f of features)for(const c of f.criteria){
    assert.equal(c.evidence.status,'passed');
    criteria.push({key:`${suiteId}/${f.id}/${c.id}`,stableKey:c.stableKey,points:c.points,passed:true,status:c.evidence.status});
  }
  const selection={...suite.selection,checks:bundle.selection.checks.filter(c=>features.some(f=>f.criteria.some(v=>v.stableKey===c.stableKey)))};
  reports.push({packRuntime:measureGradePackRuntime({...suite,features,selection})});
}
assert.deepEqual(criteria.map(c=>c.stableKey).sort(),[...keys].sort());
const packRuntime=aggregatePackRuntime(reports,snapshot.documents.execution.packs);
assert.equal(hasExactSelectedPackRuntime(packRuntime,release),true);
const manifest=snapshot.mutations.postgres;
const mutations=manifest.mutations.map(m=>mutationForRecipe(m,full)).filter(m=>{
  const targets=mutationTargetKeys(m),included=targets.filter(k=>selected.has(k));
  assert(included.length===0||included.length===targets.length,'Control spans projection boundary');
  return included.length>0;
});
const expected=new Set(mutations.map(m=>m.id)),seen=new Set(),inputs=[];
assert.equal(expected.size,158);
for(const worker of p.runs[0].workers){
  const artifactPath=resolve(root,worker.artifact),a=readArtifact(artifactPath),run=a.payload.runs[0];
  const output=resolve(dirname(artifactPath),run.output),audit=auditMutationWorkerRun(output,fixture);
  const allowed=worker.index===1?['run outcome is harness_failure','mutation control did not pass','progression-profile-address-is-discarded is INVALID_INCONCLUSIVE']:[];
  assert.deepEqual(audit.failures,allowed);
  assert.equal(run.harnessSha256Before,p.harnessSha256);assert.equal(run.harnessSha256After,p.harnessSha256);
  assert.equal(run.imageId,clean.imageId);
  const controlPath=output+'/mutation-control.json',control=readArtifact(controlPath).payload,used=[];
  for(const result of control.results){
    if(!expected.has(result.id))continue;
    assert.equal(seen.has(result.id),false);assert.equal(result.status,'CAUGHT');
    for(const field of ['targetMissing','targetHarnessFailures','targetInconclusive','targetSurvived','targetOffAssertion','setupFailures','missing'])assert.equal(result[field]?.length??0,0);
    const receipt=control.gradeReports.find(g=>g.mutationId===result.id);
    assert.equal(receipt.status,'returned');assert.equal(hash(resolve(output,receipt.report.path)),receipt.report.sha256);
    seen.add(result.id);used.push(result.id);
  }
  inputs.push({path:worker.artifact,sha256:hash(artifactPath),controlPath,controlSha256:hash(controlPath),selectedMutations:used});
}
assert.deepEqual([...seen].sort(),[...expected].sort());
const score=criteria.reduce((n,c)=>n+c.points,0);
assert.equal(score,164);
const projected={...clean,score:`${score}/${score}`,criteria:criteria.length,packRuntime,
  fingerprint:createHash('sha256').update(JSON.stringify(criteria)).digest('hex')};
for(const kind of ['reference','mutation']){
  const output=root+`/postgres-without-profile-${kind}.json`;assert.equal(fs.existsSync(output),false);
  const qualificationScope=qualificationScopeIdentity({kind,release,stack:'postgres',reference:{backend:'postgres',id:fixture.id,sourceSha256:p.fixtureSha256},
    ...(kind==='mutation'?{mutation:{backend:'postgres',executionSha256:mutationExecutionSha256({...manifest,mutations})}}:{}),stackBenchRoot:'/opt/stack-bench'});
  // This script runs in the original frozen controller, retaining its actual executable identity.
  if(kind==='mutation')assert.equal(qualificationScope.executableSha256,p.qualificationScope.executableSha256);
  const now=new Date().toISOString();
  writeRunJson(output,{...p,id:parent.id+'-without-profile-'+kind,kind:'reference_qualification',identities:parent.identities,
    startedAt:now,completedAt:now,qualifiedCheckKeys:keys,qualificationScope,mutationControl:kind==='mutation',
    runs:[{...projected,mutations:kind==='mutation'?{caught:seen.size,total:seen.size}:null}],ok:true,stable:true,sameImage:true,sameHarness:true});
  fs.copyFileSync(parentPath+'.inputs.json',output+'.inputs.json',fs.constants.COPYFILE_EXCL);
  fs.writeFileSync(output+'.derivation.json',JSON.stringify({method:'Read-only projection of the verified original reference baseline and individually caught controls, excluding the complete customer-profile scenario. No new execution or historical grade is claimed. Profile qualification is supplied separately on the fixed runtime.',
    parent:{path:parentPath,sha256:hash(parentPath)},baseline:{path:baselinePath,sha256:hash(baselinePath)},excluded,inputs,outputSha256:hash(output)},null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({output,checks:keys.length,score,controls:kind==='mutation'?seen.size:0}));
}
