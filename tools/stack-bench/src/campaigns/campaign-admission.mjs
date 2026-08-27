import { join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { readArtifact } from '../evidence/artifacts.mjs';

const SMOKE_REUSE_MS = 15 * 60_000;

function contained(root, path, label) {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is not a child of the campaign directory`);
  }
  return absolute;
}

export function validateCampaignAdmission(input, plan, directory) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    throw new Error('campaign admission payload must be an object');
  }
  const fields = new Set(['schemaVersion', 'campaignId', 'campaignSha256', 'createdAt',
    'ok', 'runtime', 'agents', 'conditions', 'reports']);
  for (const key of Object.keys(input)) {
    if (!fields.has(key)) throw new Error(`campaign admission.${key} is unknown`);
  }
  if (input.schemaVersion !== 1 || input.campaignId !== plan.id
    || input.campaignSha256 !== plan.contentSha256
    || typeof input.createdAt !== 'string' || Number.isNaN(Date.parse(input.createdAt))
    || typeof input.ok !== 'boolean') {
    throw new Error('campaign admission identity or metadata is invalid');
  }
  if (canonicalDefinitionJson(input.runtime) !== canonicalDefinitionJson(plan.definition.runtime)) {
    throw new Error('campaign admission runtime does not match the compiled plan');
  }
  const expectedAgents = plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model,
    identity: agent.identity }));
  if (canonicalDefinitionJson(input.agents) !== canonicalDefinitionJson(expectedAgents)) {
    throw new Error('campaign admission agents do not match the compiled plan');
  }
  if (canonicalDefinitionJson(input.conditions) !== canonicalDefinitionJson(plan.conditions)) {
    throw new Error('campaign admission conditions do not match the compiled plan');
  }
  if (!Array.isArray(input.reports)) throw new Error('campaign admission reports must be an array');
  const adapters = [...new Set(plan.agents.map(agent => agent.adapter))].sort();
  if (input.reports.length !== adapters.length * plan.summary.parallelism) {
    throw new Error('campaign admission reports are incomplete');
  }
  const expectedBackends = plan.stacks.map(stack => stack.id);
  const expectedResultsDir = resolve(directory);
  for (const adapter of adapters) {
    for (let runIndex = 0; runIndex < plan.summary.parallelism; runIndex += 1) {
      const matches = input.reports.filter(report => report?.request?.agentAdapter === adapter
        && report?.request?.runIndex === runIndex);
      if (matches.length !== 1) {
        throw new Error(`campaign admission must contain one ${adapter} report for run slot ${runIndex}`);
      }
      const report = matches[0];
      if (report.schemaVersion !== 1 || typeof report.ok !== 'boolean' || !Array.isArray(report.checks)
        || !report.summary || typeof report.summary !== 'object'
        || !report.checks.every(check => check && typeof check === 'object'
          && typeof check.id === 'string' && ['pass', 'warn', 'fail'].includes(check.status)
          && typeof check.summary === 'string')
        || report.summary.passed !== report.checks.filter(check => check.status === 'pass').length
        || report.summary.failed !== report.checks.filter(check => check.status === 'fail').length
        || report.summary.warnings !== report.checks.filter(check => check.status === 'warn').length
        || report.ok !== !report.checks.some(check => check.status === 'fail')) {
        throw new Error(`campaign admission report for ${adapter} is malformed`);
      }
      const request = report.request;
      if (canonicalDefinitionJson(request.backends) !== canonicalDefinitionJson(expectedBackends)
        || request.track !== plan.definition.track
        || canonicalDefinitionJson(request.levels) !== canonicalDefinitionJson(plan.definition.levels)
        || request.runIndex !== runIndex
        || canonicalDefinitionJson(request.packs)
          !== canonicalDefinitionJson(plan.definition.selection.packs ?? [])
        || canonicalDefinitionJson(request.checks)
          !== canonicalDefinitionJson(plan.definition.selection.checks ?? [])
        || request.smoke !== true
        || (plan.definition.runtime.buildImage !== null
          && request.image !== plan.definition.runtime.buildImage)
        || resolve(request.resultsDir) !== expectedResultsDir) {
        throw new Error(`campaign admission report for ${adapter} does not match the compiled scope`);
      }
    }
  }
  if (input.ok !== input.reports.every(report => report.ok)) {
    throw new Error('campaign admission verdict does not match its reports');
  }
  return structuredClone(input);
}

export function readCampaignAdmission(directory, id, plan) {
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  const artifact = readArtifact(path, { expectedKind: 'campaign_admission', expectedId: id });
  if (artifact.identities.experiment?.sha256 !== plan.contentSha256) {
    throw new Error(`campaign admission ${id} has the wrong experiment identity`);
  }
  return validateCampaignAdmission(artifact.payload, plan, directory);
}

export function campaignAdmissionSmokeReuse(admission, request,
  { now = Date.now(), maxAgeMs = SMOKE_REUSE_MS } = {}) {
  if (admission.ok !== true) throw new Error('campaign admission did not pass');
  const reports = admission.reports.filter(report =>
    report.request.agentAdapter === request.agentAdapter
    && report.request.runIndex === request.runIndex);
  if (reports.length !== 1) throw new Error('campaign admission has no exact attempt report');
  const report = reports[0];
  if (report.ok !== true) throw new Error('campaign admission attempt report did not pass');
  if (!report.request.backends.includes(request.backend)) {
    throw new Error(`campaign admission does not cover stack ${request.backend}`);
  }
  if (report.request.image !== request.image) {
    return { reusable: false, reason: 'build image changed after campaign admission' };
  }
  const createdAt = Date.parse(admission.createdAt);
  const ageMs = now - createdAt;
  if (!Number.isFinite(ageMs) || ageMs < -60_000 || ageMs > maxAgeMs) {
    return { reusable: false, reason: 'campaign admission is not recent' };
  }
  const smoke = report.checks.find(check => check.id === 'smoke.container');
  if (smoke?.status !== 'pass') {
    return { reusable: false, reason: 'campaign admission has no passing container smoke check' };
  }
  return { reusable: true, reason: null, createdAt: admission.createdAt };
}
