import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {loadTrack,portsFor} from '../../dist/src/composition/tracks.js';
import {requireRecipeRelease} from '../../dist/src/composition/recipe-release.js';
import {resolveFeatureCatalog} from '../../dist/src/progression/feature-catalog-selection.js';
import {resolveProgressionRecipeLevelSelection} from '../../dist/src/progression/progression-recipe-selection.js';
import {resolveBoundRecipeTaskRequest} from '../../dist/src/composition/recipe-selection.js';
import {resolveGuidanceProfile} from '../../dist/src/campaigns/condition-compiler.js';
import {readAgentSkillDocuments} from '../../dist/src/agents/agent-materials.js';
import {buildPrompt,parseAgentArgs} from '../../dist/commands/agent.js';
import {STACK_BENCH_ROOT} from '../../dist/src/package-root.js';
const stacks=['spacetime','postgres','mongodb','convex'];
const track=loadTrack('ecommerce'),catalog=resolveFeatureCatalog('progression/ecommerce.json',track);
const guidance=resolveGuidanceProfile('neutral-dev',stacks),rows=[];
const inputPaths=fs.readdirSync('tracks/ecommerce/contracts').filter(p=>p.endsWith('.md')).map(p=>'tracks/ecommerce/contracts/'+p);
const hash=s=>createHash('sha256').update(s).digest('hex');
const inputs=inputPaths.map(path=>({path,sha256:hash(fs.readFileSync(path))}));
for(const level of [1,2,3]){
 const binding=requireRecipeRelease(track,level,'ecommerce.progression-catalog');
 const task=resolveProgressionRecipeLevelSelection(binding,catalog,level,{cumulative:true}).agent.request;
 const selected=resolveBoundRecipeTaskRequest(binding,task);
 for(const stack of stacks){
  const args=parseAgentArgs(['node','agent','--mode',level===1?'build':'upgrade','--backend',stack,'--track','ecommerce','--level',String(level),'--run-index','0','--app','/prompt-review/app','--guidance','neutral','--guidance-document-json',JSON.stringify(guidance.documents[stack]),'--credential-aliases-json',JSON.stringify(guidance.credentialAliases),'--skill-identity-json',JSON.stringify(guidance.skills[stack]),'--recipe-task-json',JSON.stringify(task)]);
  const prompt=buildPrompt(args,portsFor(track,stack,0),track,{skillsText:readAgentSkillDocuments(STACK_BENCH_ROOT,guidance.skills[stack].ids),requirementText:selected.task.requirementText,contractText:selected.task.contractText,startingCatalog:JSON.stringify({warehouses:binding.plan.fixture.warehouses,items:binding.plan.fixture.items},null,2)});
  const marker='On that `support-ticket`, expose `data-submit-state` for the latest update:';
  assert.equal(prompt.includes(marker),level===2);
  assert(!prompt.includes('awaitWrite')&&!prompt.includes('611a')&&!prompt.includes('Missing item'));
  const roleTab=prompt.includes('`staff-roles-link`'),productTab=prompt.includes('`catalog-management-link`');
  assert.equal(roleTab,level===2);assert.equal(productTab,level===3);
  assert(prompt.includes('Build a production-quality application suitable for real users, not a prototype or demo.'));
  assert(!/unlessVisible|expectReplayRejected|gradeFeature|mutation control|ACCESS10|HACK10|Volume Item 0999/.test(prompt));
  const file=`local-notes/sol6-four-stack-20260925/ready-navigation-prompt-${stack}-l${level}.txt`;
  fs.writeFileSync(file,prompt);
  rows.push({stack,level,path:file,sha256:hash(prompt),supportReceipt:prompt.includes(marker),roleTab,productTab,mode:level===1?'build':'upgrade',guidance:'neutral-dev',skills:guidance.skills[stack].ids,recipeContentSha256:binding.release.contentSha256,requestSha256:hash(JSON.stringify(task)),requirementSha256:hash(selected.task.requirementText),contractSha256:hash(selected.task.contractText),productionQuality:true});
 }
}
assert(inputs.every(i=>i.sha256===hash(fs.readFileSync(i.path))));
fs.writeFileSync('local-notes/sol6-four-stack-20260925/ready-navigation-prompt-review.json',JSON.stringify({ok:true,inputs,rows,limits:'Scoped normal build/upgrade prompt rendering for no-repair progression; no model execution or qualification claim. No repair prompt rendered.'},null,2)+'\n');
console.log(JSON.stringify({ok:true,prompts:rows.length}));

