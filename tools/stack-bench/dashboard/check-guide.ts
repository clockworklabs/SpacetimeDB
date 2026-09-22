import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { loadTrack, listTracks } from '../src/composition/tracks.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { progressionLevels, selectFeatureCatalogLevels } from '../src/progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection } from '../src/progression/progression-recipe-selection.js';
import type { CompiledCriterion, CompiledFeature } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { stepsText, type GuideStep } from './check-guide-steps.js';
import { esc } from './public/format.js';
import { topbar } from './public/views/plans.js';

interface Check {
  key: string;
  active: boolean;
  source: string;
  sourceSha256: string;
  feature: CompiledFeature;
  criterion: CompiledCriterion;
  controls: string[];
}

function inventory() {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
  if (!binding) throw new Error('The ecommerce dependency recipe is unavailable.');
  const full = resolveFeatureCatalog('progression/ecommerce.json', track);
  const catalog = selectFeatureCatalogLevels(full, progressionLevels(full).filter(level => level <= 3));
  const selected = new Set(resolveProgressionRecipeLevelSelection(binding, catalog, 3,
    { cumulative: true }).grader.checkKeys);
  const mutationRoot = join(STACK_BENCH_ROOT, 'grader', 'mutations');
  const mutations = readdirSync(mutationRoot).filter(file => file.endsWith('-ecommerce.json')).flatMap(file => {
    const manifest = JSON.parse(readFileSync(join(mutationRoot, file), 'utf8')) as {
      mutations: { id: string; desc: string; targets?: string[] }[];
    };
    return manifest.mutations.map(mutation => ({ ...mutation, backend: file.replace('-ecommerce.json', '') }));
  });
  const sources = new Map<string, { features: CompiledFeature[]; sha256: string }>();
  const read = (source: string) => {
    let value = sources.get(source);
    if (!value) {
      const text = readFileSync(join(STACK_BENCH_ROOT, source), 'utf8');
      value = { ...JSON.parse(text), sha256: createHash('sha256').update(text).digest('hex') };
      sources.set(source, value!);
    }
    return value!;
  };
  const checks: Check[] = binding.release.checkCatalog.map(check => {
    const source = `tracks/ecommerce/${check.source}`;
    const scenario = read(source);
    const feature = scenario.features.find(feature => feature.id === check.featureId);
    const criterion = feature?.criteria.find(criterion => criterion.id === check.criterionId);
    if (!feature || !criterion) throw new Error(`Missing source for ${check.stableKey}`);
    return { key: check.stableKey, active: selected.has(check.stableKey), source,
      sourceSha256: scenario.sha256, feature, criterion,
      controls: mutations.filter(mutation => mutation.targets?.includes(check.stableKey))
        .map(mutation => `${mutation.backend}: ${mutation.desc} (${mutation.id})`) };
  });
  const included = new Set(checks.map(c => `${c.source}:${c.feature.id}:${c.criterion.id}`));
  for (const name of listTracks()) {
    const other = loadTrack(name);
    for (const file of readdirSync(other.scenarios).filter(file => file.endsWith('.json')).sort()) {
      const source = `tracks/${name}/scenarios/${file}`;
      const scenario = read(source);
      for (const feature of scenario.features) for (const criterion of feature.criteria) {
        const key = `${source}:${feature.id}:${criterion.id}`;
        if (!included.has(key)) checks.push({ key, active: false, source,
          sourceSha256: scenario.sha256, feature, criterion, controls: [] });
      }
    }
  }
  return { checks, selected: selected.size, recipe: binding.release.id,
    sha256: binding.release.contentSha256 };
}

function procedure(steps: GuideStep[]): string {
  return stepsText(steps).map(line => {
    const text = line.trimStart();
    const kind = /Repeat \d|branches concurrently|Branch \d/.test(text) ? 'repeat'
      : /^(?:- )?(?:Wait |Always wait|Observation deadline)/.test(text) ? 'wait'
      : /require|reject|check that|check for|check all|compare stored|compare the|inspect stored/i.test(text) ? 'assert'
      : 'action';
    const indent = Math.min(4, (line.length - text.length) / 2);
    return `<div class="guide-step guide-indent-${indent}"><span class="guide-kind ${kind}">${
      { repeat: 'Repeat / parallel', wait: 'Wait / deadline', assert: 'Verify', action: 'Action' }[kind]
    }</span><span>${esc(text.replace(/^- /, ''))}</span></div>`;
  }).join('') || '<p class="guide-note">No additional steps in this section.</p>';
}

function entry(check: Check): string {
  const { feature, criterion } = check;
  const search = `${check.key} ${criterion.desc} ${feature.name} ${check.source}`.toLowerCase();
  return `<details class="guide-check" data-key="${esc(check.key)}" data-active="${check.active}" data-search="${esc(search)}">`
    + `<summary><span class="guide-id">${esc(criterion.id)}</span><span>${esc(criterion.desc)}</span>`
    + `<span class="guide-scope">${check.active ? 'L1–L3' : 'Other'}</span></summary><div class="guide-body">`
    + `<p class="guide-key">${esc(check.key)}</p><p>${criterion.points} ${criterion.points === 1 ? 'point' : 'points'} · Browsers: ${esc((feature.actors ?? []).join(', ') || 'none')}</p>`
    + (criterion.statedBy ? `<p><b>Product rule:</b> ${esc(criterion.statedBy)}</p>` : '')
    + (criterion.note ? `<p class="guide-note"><b>Note:</b> ${esc(criterion.note)}</p>` : '')
    + (criterion.provenBy ? `<p><b>Uses earlier proof:</b> ${esc(criterion.provenBy)}</p>` : '')
    + '<h2>Setup</h2>' + procedure((feature.setup ?? []) as GuideStep[])
    + '<h2>Check steps</h2>' + procedure(criterion.steps as GuideStep[])
    + '<div class="guide-pass"><b>Pass rule</b><p>All required actions and assertions must succeed. '
    + 'A failed prerequisite blocks dependent work. Unknown results and harness errors are not passes. '
    + 'Setup and earlier criteria can be shared; the runner preserves their prerequisite order.</p></div>'
    + '<details class="guide-evidence"><summary>Defect controls</summary>'
    + (check.controls.length ? '<p>Declared control targets, not proof of a passing qualification run.</p><ul>'
      + check.controls.map(control => `<li>${esc(control)}</li>`).join('') + '</ul>'
      : '<p>No exact control target is linked here. Check calibration evidence for broader controls.</p>')
    + '</details><details class="guide-evidence"><summary>Exact setup and check input</summary>'
    + `<p class="guide-key">${esc(check.source)} · SHA-256 ${check.sourceSha256}</p>`
    + `<pre>${esc(JSON.stringify({ actors: feature.actors, setup: feature.setup ?? [], criterion }, null, 2))}</pre>`
    + '</details></div></details>';
}

/** Current local definitions only. Historical campaign evidence retains its frozen version. */
export function checkGuidePage(): string {
  const data = inventory();
  return '<!doctype html><html lang="en"><head><meta charset="utf-8">'
    + '<meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="dark">'
    + '<title>Checks · Stack Bench</title><link rel="stylesheet" href="/styles.css">'
    + '<script type="module" src="/check-guide.js"></script></head><body>'
    + topbar({ page: 'check-guide', key: '', canStart: false, resumable: false, error: '' })
    + '<main class="page guide"><div class="title"><h1>Checks</h1></div>'
    + `<p class="guide-intro">What each check does, step by step. <b>${data.selected} checks</b> in the current ecommerce dependency L1–L3 selection.</p>`
    + '<details class="guide-help"><summary>Scope and timing</summary><p>This page reads the local definitions. It does not describe a frozen historical run or certify qualification. Other selections can overlap and are not extra checks in L1–L3.</p>'
    + '<p>Loops keep their repetition counts. The exact input includes every field. Shown waits and deadlines are authored values; omitted deadlines use grader defaults. Helpers can also wait or issue several requests. Setup can be shared, so these steps are not an elapsed-time estimate.</p>'
    + `<p class="guide-key">Recipe: ${esc(data.recipe)} · SHA-256 ${data.sha256}</p></details>`
    + '<div class="guide-toolbar"><label class="guide-search">Search checks<input type="search" data-guide-search placeholder="Catalog, stock, password, crash…"></label>'
    + '<div class="guide-controls"><label><input type="checkbox" data-guide-others> Include other selections</label>'
    + '<button class="btn" data-guide-expand>Expand visible</button><button class="btn" data-guide-collapse>Collapse all</button>'
    + '<span data-guide-count role="status"></span></div></div>'
    + '<div class="guide-legend" aria-label="Step labels"><span class="guide-kind action">Action</span>'
    + '<span class="guide-kind assert">Verify</span><span class="guide-kind wait">Wait / deadline</span><span class="guide-kind repeat">Repeat / parallel</span></div>'
    + '<p data-guide-empty hidden>No checks match this search.</p>'
    + data.checks.map(entry).join('') + '</main></body></html>';
}
