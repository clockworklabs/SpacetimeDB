// Read-only diagnostic: expose the existing scope walk without changing its owner.
import fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
const [rootArg, evidenceArg] = process.argv.slice(2);
const root = resolve(rootArg), evidence = resolve(evidenceArg);
const modulePath = resolve(root, 'dist/src/composition/qualification-scope.js');
const require = createRequire(modulePath);
let source = fs.readFileSync(modulePath, 'utf8').replace(/from (['"])([^'"]+)\1/g, (_, quote, spec) =>
  `from ${JSON.stringify(spec.startsWith('node:') ? spec : pathToFileURL(spec.startsWith('.') ? resolve(dirname(modulePath), spec) : require.resolve(spec)).href)}`);
source += `\nexport function diagnosticInputs(root, kind, stack, release) {
 const files = moduleGraph(root, [...KIND_ENTRYPOINTS[kind], ...(stack === null ? [] : stackAssets(root, stack).map(path => relative(root, path)))], {stack});
 files.push(...RUNTIME_INPUTS.map(path => resolve(root,path)), resolve(root,'tracks',release.track,'walk.ts'));
 return {files:[...new Set(files)].map(path => ({path:relative(root,path).replaceAll('\\\\','/'),sha256:sha256(readFileSync(path,'utf8').replaceAll('\\r\\n','\\n')), ...(REGISTRY_MODULES.has(relative(root,path).replaceAll('\\\\','/')) ? {projectionSha256:sha256(registryProjection(path,root,stack))} : {})})).sort((a,b)=>a.path.localeCompare(b.path)),interfaceSha256:stack===null?null:sha256(stackInterfaceText(root,release,stack))};
}\n`;
const { qualificationScopeIdentity, diagnosticInputs } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const read = path => JSON.parse(fs.readFileSync(path,'utf8'));
const sha = path => createHash('sha256').update(fs.readFileSync(path)).digest('hex');
const rows = [];
for (const stack of ['spacetime','postgres','mongodb','convex',null]) {
 const path=resolve(evidence,stack?`${stack}-controls.json`:'null.json');
 const raw=read(path), artifact=raw.payload??raw;
 const snapshot=read(`${path}.inputs.json`);
 const release={...snapshot.documents.release,checkCatalog:snapshot.documents.release.checkCatalog.filter(c=>artifact.qualificationScope.checksSha256 && (artifact.qualifiedCheckKeys??artifact.criteria.map(r=>snapshot.documents.release.checkCatalog.find(c=>c.source===r.scenario&&c.featureId===r.feature&&c.criterionId===r.criterion)?.stableKey)).includes(c.stableKey))};
 const reference=stack?snapshot.calibration.references.entries.find(r=>r.backend===stack):null;
 for(const kind of stack?['reference','mutation']:['null']) {
  const input={kind,stack,reference,mutation:kind==='mutation'?{backend:stack,executionSha256:artifact.qualificationScope.mutationSha256}:null,release,stackBenchRoot:root};
  rows.push({kind,stack,scope:qualificationScopeIdentity(input),...diagnosticInputs(root,kind,stack,release)});
 }
}
console.log(JSON.stringify({schemaVersion:1,scopeModuleSha256:sha(modulePath),compilerSourceSha256:sha(resolve(root,'src/composition/calibration-compiler.ts')),compilerRuntimeSha256:sha(resolve(root,'dist/src/composition/calibration-compiler.js')),rows},null,2));
