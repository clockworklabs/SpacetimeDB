import fs from 'node:fs';
import {dirname,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {readArtifact,writeRunJson} from '/opt/stack-bench/dist/src/evidence/artifacts.js';
import {referenceRunFromMutationBaseline} from '/opt/stack-bench/dist/src/references/reference-live.js';
import {loadReferenceRegistry,selectReferenceFixture} from '/opt/stack-bench/dist/src/references/reference-fixtures.js';
import {buildRecipeRelease} from '/opt/stack-bench/dist/src/composition/recipe-release.js';
import {qualificationScopeIdentity} from '/opt/stack-bench/dist/src/composition/qualification-scope.js';
const parent=process.env.STACK_BENCH_RESULTS_DIR+'/diagnostics/role-revocation-20260926/mongodb-remaining-final-2.json';
const output=parent.replace('remaining-final-2.json','remaining-reference.json');
if(fs.existsSync(output))throw Error('Refusing to replace derived evidence');
const source=readArtifact(parent,{expectedKind:'reference_qualification'}),p=source.payload;
const fixture=selectReferenceFixture(loadReferenceRegistry(),{backend:'mongodb',track:'ecommerce',level:3,recipe:'ecommerce.progression-catalog'});
const full=buildRecipeRelease(resolve('/opt/stack-bench/tracks/ecommerce/composition/recipes/progression-catalog.json'),{trackRoot:resolve('/opt/stack-bench/tracks/ecommerce')});
const selected=new Set(p.qualifiedCheckKeys),release={...full,checkCatalog:full.checkCatalog.filter(c=>selected.has(c.stableKey))};
const run=referenceRunFromMutationBaseline(dirname(parent),p.runs[0],fixture,{release:full,level:3,selectedCheckKeys:p.qualifiedCheckKeys});
if(!run.ok||run.score!=='169/169'||!run.fingerprint||!run.imageId||run.harnessSha256Before!==run.harnessSha256After)throw Error(JSON.stringify(run.failures));
const now=new Date().toISOString();
const artifact={...p,id:source.id+'-verified-baseline',kind:'reference_qualification',identities:source.identities,
 startedAt:now,completedAt:now,mutationControl:false,runs:[run],ok:true,stable:true,sameImage:true,sameHarness:true,
 harnessSha256:run.harnessSha256Before,diagnostic:true,
 qualificationScope:qualificationScopeIdentity({kind:'reference',release,stack:'mongodb',reference:{backend:'mongodb',id:fixture.id,sourceSha256:p.fixtureSha256},stackBenchRoot:'/opt/stack-bench'})};
const derivation={path:parent,sha256:createHash('sha256').update(fs.readFileSync(parent)).digest('hex'),method:'Existing referenceRunFromMutationBaseline auditor; original baseline reused, no new reference execution. Failed worker qualification remains unchanged.'};
writeRunJson(output,artifact);
fs.writeFileSync(output+'.derivation.json',JSON.stringify({...derivation,outputSha256:createHash('sha256').update(fs.readFileSync(output)).digest('hex')},null,2)+'\n',{flag:'wx'});
fs.copyFileSync(parent+'.inputs.json',output+'.inputs.json',fs.constants.COPYFILE_EXCL);
console.log(JSON.stringify({output,score:run.score,checks:p.qualifiedCheckKeys.length,ok:run.ok}));
