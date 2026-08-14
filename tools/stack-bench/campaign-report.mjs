#!/usr/bin/env node

import { randomUUID } from 'node:crypto';
import { mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { emptyArtifactIdentities, readArtifactPayload, writeArtifact } from './artifacts.mjs';
import { inspectCampaign, validateCampaignRun } from './campaign-runner.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.mjs';
import { sha256 } from './provenance.mjs';

export const CAMPAIGN_REPORT_SCHEMA_VERSION = 1;

const number = value => Number.isFinite(value) ? value : null;
const ratio = (value, max) => Number.isFinite(value) && Number.isFinite(max) && max > 0
  ? Number((value / max).toFixed(6)) : null;
const mean = values => values.reduce((total, value) => total + value, 0) / values.length;

export function formatDurationMs(value) {
  if (!Number.isFinite(value) || value < 0) return '—';
  const totalSeconds = Math.round(value / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours) return `${hours}h ${minutes}m ${seconds}s`;
  if (minutes) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

function formatUsd(value) {
  return Number.isFinite(value) ? `$${Number(value.toFixed(4))}` : '—';
}

function quantile(sorted, p) {
  if (sorted.length === 1) return sorted[0];
  const index = (sorted.length - 1) * p;
  const low = Math.floor(index);
  const high = Math.ceil(index);
  return sorted[low] + (sorted[high] - sorted[low]) * (index - low);
}

function summarize(values, dispersion) {
  const present = values.filter(Number.isFinite).sort((a, b) => a - b);
  if (!present.length) return { n: 0, center: null, spread: null, min: null, max: null };
  if (dispersion === 'median-iqr') return {
    n: present.length,
    center: Number(quantile(present, 0.5).toFixed(6)),
    spread: { kind: 'iqr', q1: Number(quantile(present, 0.25).toFixed(6)),
      q3: Number(quantile(present, 0.75).toFixed(6)) },
    min: present[0], max: present.at(-1),
  };
  const center = mean(present);
  const variance = present.length > 1
    ? present.reduce((total, value) => total + ((value - center) ** 2), 0) / (present.length - 1)
    : 0;
  return { n: present.length, center: Number(center.toFixed(6)),
    spread: { kind: 'sd', value: Number(Math.sqrt(variance).toFixed(6)) },
    min: present[0], max: present.at(-1) };
}

function metrics(run) {
  const levels = run.levels ?? [];
  const completeFirstBuild = levels.length > 0 && levels.every(level =>
    number(level.firstBuild?.score) !== null && number(level.firstBuild?.max) !== null);
  const firstScore = completeFirstBuild
    ? levels.reduce((total, level) => total + level.firstBuild.score, 0) : null;
  const firstMax = completeFirstBuild
    ? levels.reduce((total, level) => total + level.firstBuild.max, 0) : null;
  return {
    firstBuildScoreRate: ratio(firstScore, firstMax),
    finalScoreRate: ratio(number(run.totals?.score), number(run.totals?.max)),
    totalCostUsd: number(run.totals?.costUsd),
    totalDurationMs: number(run.totals?.durationSec) === null ? null : run.totals.durationSec * 1000,
    fixRounds: number(run.totals?.fixRounds),
  };
}

function conditionKey(attempt) {
  return canonicalDefinitionJson({ stack: attempt.stack, agentAdapter: attempt.agentAdapter,
    model: attempt.model, guidance: attempt.guidance, skills: attempt.skills }).trim();
}

function exactFields(value, fields, at) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${at} must be an object`);
  }
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
}

export function validateCampaignReport(input) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    throw new Error('campaign report must be an object');
  }
  const fields = new Set(['reportSchemaVersion', 'campaign', 'scope', 'policy', 'attempts',
    'conditions', 'summary', 'limitations', 'contentSha256']);
  for (const key of Object.keys(input)) if (!fields.has(key)) throw new Error(`campaign report.${key} is unknown`);
  if (input.reportSchemaVersion !== CAMPAIGN_REPORT_SCHEMA_VERSION
    || !input.campaign || typeof input.campaign !== 'object'
    || !/^[a-f0-9]{64}$/.test(input.campaign.sha256)
    || !Array.isArray(input.attempts) || !Array.isArray(input.conditions)
    || !Array.isArray(input.limitations) || !input.summary || typeof input.summary !== 'object') {
    throw new Error('campaign report structure is invalid');
  }
  exactFields(input.campaign, new Set(['id', 'version', 'state', 'sha256', 'title']),
    'campaign report.campaign');
  exactFields(input.scope, new Set(['track', 'levels', 'selection', 'bindings', 'stacks',
    'agents', 'repetitions', 'runtime', 'pricing']), 'campaign report.scope');
  exactFields(input.policy, new Set(['primaryMetric', 'secondaryMetrics', 'dispersion',
    'invalidAttempts', 'missingData', 'comparisonUnit']), 'campaign report.policy');
  if (typeof input.campaign.id !== 'string' || !input.campaign.id
    || typeof input.campaign.title !== 'string' || !input.campaign.title
    || typeof input.scope.track !== 'string' || !input.scope.track
    || !Array.isArray(input.scope.levels) || !Array.isArray(input.scope.bindings)
    || !Array.isArray(input.scope.stacks) || !Array.isArray(input.scope.agents)
    || !Number.isInteger(input.scope.repetitions) || input.scope.repetitions < 1
    || !input.scope.runtime || typeof input.scope.runtime !== 'object'
    || !input.scope.pricing || typeof input.scope.pricing !== 'object') {
    throw new Error('campaign report exact scope is invalid');
  }
  const { contentSha256, ...body } = canonicalizeDefinition(input);
  if (typeof contentSha256 !== 'string'
    || contentSha256 !== sha256(canonicalDefinitionJson(body))) {
    throw new Error('campaign report content identity is invalid');
  }
  return canonicalizeDefinition(input);
}

export function buildCampaignReport(plan, state, readRun) {
  if (state.campaignSha256 !== plan.contentSha256) throw new Error('report state does not match campaign plan');
  const rows = [];
  for (const attempt of state.attempts) {
    const executions = attempt.executions.map(execution => {
      let run = null;
      if (execution.status === 'completed') {
        run = readRun(attempt.plan, execution);
        if (!run || run.outcome?.kind !== execution.outcome) {
          throw new Error(`completed execution ${execution.id} has missing or mismatched run evidence`);
        }
      }
      return {
        id: execution.id, ordinal: execution.ordinal, status: execution.status,
        outcome: execution.outcome, reason: execution.reason, exitCode: execution.exitCode,
        startedAt: execution.startedAt, completedAt: execution.completedAt,
        admissionId: execution.admissionId,
        admissionEvidence: `admissions/${execution.admissionId}.json`,
        evidence: execution.status === 'completed' ? `${execution.output}/run.json` : 'state.json',
        metrics: run ? metrics(run) : null,
      };
    });
    rows.push({ ...attempt.plan, status: attempt.status, executions,
      metrics: executions.at(-1)?.status === 'completed' ? executions.at(-1).metrics : null });
  }
  const metricNames = [plan.definition.analysis.primaryMetric,
    ...plan.definition.analysis.secondaryMetrics].filter(metric => metric !== 'invalidAttemptRate');
  const groups = new Map();
  for (const row of rows) {
    const key = conditionKey(row);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(row);
  }
  const conditions = [...groups.entries()].map(([key, attempts]) => {
    const completed = attempts.filter(attempt => attempt.status === 'completed');
    const executions = attempts.flatMap(attempt => attempt.executions);
    const invalidExecutions = executions.filter(execution => execution.status === 'invalid').length;
    return {
      key: sha256(key),
      stack: attempts[0].stack,
      agent: { adapter: attempts[0].agentAdapter, model: attempts[0].model,
        guidance: attempts[0].guidance, skills: attempts[0].skills },
      sample: { plannedAttempts: attempts.length, completedAttempts: completed.length,
        invalidAttempts: attempts.filter(attempt => attempt.status === 'invalid').length,
        pendingAttempts: attempts.filter(attempt => attempt.status === 'pending').length,
        executions: executions.length, invalidExecutions,
        invalidExecutionRate: executions.length
          ? Number((invalidExecutions / executions.length).toFixed(6)) : 0 },
      metrics: { ...Object.fromEntries(metricNames.map(metric => [metric,
        summarize(completed.map(attempt => attempt.metrics?.[metric]),
          plan.definition.analysis.dispersion)])),
      invalidAttemptRate: { n: attempts.length,
        center: Number((attempts.filter(attempt => attempt.status === 'invalid').length
          / attempts.length).toFixed(6)), spread: null, min: null, max: null } },
    };
  }).sort((left, right) => left.key.localeCompare(right.key));
  const executions = rows.flatMap(attempt => attempt.executions);
  const invalidExecutions = executions.filter(execution => execution.status === 'invalid').length;
  const body = canonicalizeDefinition({
    reportSchemaVersion: CAMPAIGN_REPORT_SCHEMA_VERSION,
    campaign: { id: plan.id, version: plan.version, state: plan.state,
      sha256: plan.contentSha256, title: plan.title },
    scope: { track: plan.definition.track, levels: plan.definition.levels,
      selection: plan.definition.selection, bindings: plan.bindings, stacks: plan.stacks,
      agents: plan.agents, repetitions: plan.definition.repetitions,
      runtime: plan.definition.runtime, pricing: plan.definition.pricing },
    policy: plan.definition.analysis,
    attempts: rows,
    conditions,
    summary: { campaignStatus: state.status, plannedAttempts: rows.length,
      completedAttempts: rows.filter(attempt => attempt.status === 'completed').length,
      invalidAttempts: rows.filter(attempt => attempt.status === 'invalid').length,
      invalidAttemptRate: rows.length
        ? Number((rows.filter(attempt => attempt.status === 'invalid').length / rows.length).toFixed(6)) : 0,
      pendingAttempts: rows.filter(attempt => attempt.status === 'pending').length,
      runningAttempts: rows.filter(attempt => attempt.status === 'running').length,
      executions: executions.length, invalidExecutions,
      invalidExecutionRate: executions.length
        ? Number((invalidExecutions / executions.length).toFixed(6)) : 0 },
    limitations: [
      'Statistics describe only the exact scope and conditions recorded above.',
      'Invalid executions are reported separately and are not imputed into outcome metrics.',
      'The report makes no causal claim beyond the declared campaign design.',
    ],
  });
  return validateCampaignReport({ ...body, contentSha256: sha256(canonicalDefinitionJson(body)) });
}

function escape(value) {
  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
}

export function renderCampaignHtml(report, { evidencePrefix = '..' } = {}) {
  report = validateCampaignReport(report);
  const rows = report.conditions.map(condition => `<tr><td>${escape(condition.stack)}</td>`
    + `<td>${escape(condition.agent.adapter)} / ${escape(condition.agent.model)}</td>`
    + `<td>${condition.sample.completedAttempts}/${condition.sample.plannedAttempts}</td>`
    + `<td>${condition.sample.invalidExecutions}/${condition.sample.executions}</td>`
    + `<td>${escape(condition.metrics[report.policy.primaryMetric]?.center ?? '—')}`
    + `<br><small>${escape(formatUsd(condition.metrics.totalCostUsd?.center))} normalized usage · `
    + `${escape(formatDurationMs(condition.metrics.totalDurationMs?.center))}</small></td></tr>`).join('');
  return `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escape(report.campaign.title)}</title><style>body{font:16px system-ui;max-width:1100px;margin:40px auto;padding:0 20px;color:#17202a}code{font-size:.85em}table{border-collapse:collapse;width:100%}th,td{padding:.65rem;border-bottom:1px solid #ccd;text-align:left}.meta{color:#566} .warn{background:#fff4cf;padding:1rem}</style></head><body><h1>${escape(report.campaign.title)}</h1><p class="meta">Campaign <code>${escape(report.campaign.id)}</code> · ${escape(report.campaign.sha256)} · status ${escape(report.summary.campaignStatus)}</p><p>This report shows exactly what ran: ${report.summary.completedAttempts} completed of ${report.summary.plannedAttempts} planned attempts, with ${report.summary.invalidExecutions} invalid execution(s) retained.</p><h2>Conditions</h2><table><thead><tr><th>Stack</th><th>Agent / model</th><th>Completed</th><th>Invalid executions</th><th>${escape(report.policy.primaryMetric)}</th></tr></thead><tbody>${rows}</tbody></table><h2>Scope</h2><pre>${escape(JSON.stringify(report.scope, null, 2))}</pre><h2>Attempts and raw evidence</h2><ul>${report.attempts.map(attempt => `<li><strong>${escape(attempt.id)}</strong> — ${escape(attempt.status)}${attempt.executions.map(execution => ` · <a href="${escape(`${evidencePrefix}/${execution.evidence}`)}">${escape(execution.id)}</a> (${escape(execution.outcome ?? execution.status)}) · <a href="${escape(`${evidencePrefix}/${execution.admissionEvidence}`)}">admission</a>`).join('')}</li>`).join('')}</ul><div class="warn"><strong>Limitations</strong><ul>${report.limitations.map(item => `<li>${escape(item)}</li>`).join('')}</ul></div><p class="meta">Report identity: <code>${escape(report.contentSha256)}</code></p></body></html>\n`;
}

export function generateCampaignReport(directory, { output = join(resolve(directory), 'report') } = {}) {
  const { plan, state, paths } = inspectCampaign(directory);
  if (state.status === 'running') throw new Error('cannot report while a campaign attempt is running');
  output = resolve(output);
  const outputRelative = relative(paths.root, output);
  if (outputRelative === '' || outputRelative === '..' || outputRelative.startsWith(`..${sep}`)) {
    throw new Error('report output must be a child of the campaign directory');
  }
  const report = buildCampaignReport(plan, state, (_attempt, execution) =>
    validateCampaignRun(plan, _attempt,
      readArtifactPayload(join(paths.root, execution.output, 'run.json'), { expectedKind: 'benchmark_run' })));
  mkdirSync(output, { recursive: true });
  const reportPath = join(output, 'report.json');
  writeArtifact(reportPath, { kind: 'campaign_report', id: `${plan.id}-report-${report.contentSha256.slice(0, 16)}`,
    timestamps: { startedAt: state.createdAt, completedAt: state.updatedAt },
    identities: emptyArtifactIdentities({ experiment: {
      id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
    } }), payload: report });
  const htmlPath = join(output, 'report.html');
  const evidencePrefix = relative(output, paths.root).replaceAll('\\', '/') || '.';
  const temporaryHtml = `${htmlPath}.${process.pid}.${randomUUID()}.tmp`;
  writeFileSync(temporaryHtml, renderCampaignHtml(report, { evidencePrefix }), { flag: 'wx' });
  renameSync(temporaryHtml, htmlPath);
  return { report, reportPath, htmlPath,
    relativeOutput: outputRelative.replaceAll('\\', '/') };
}
