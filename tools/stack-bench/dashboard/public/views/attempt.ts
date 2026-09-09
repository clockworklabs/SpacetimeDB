import type { TranscriptPage } from '../../dashboard-transcript.js';
// One attempt: figures, the climb at full size, and the evidence behind tabs.
// Each tab is a link, so what is open survives a reload and a back button.

import type { AttemptCheck, AttemptChecks, AttemptPackage, CampaignSheet, SheetAttempt, SheetStack }
  from '../../dashboard-views.js';
import type { readCampaignTimeBudget } from '../../../src/campaigns/campaign-time-grant.js';
import { bigClimb } from '../climb.js';
import { DASH, duration, executionClock, esc, metricLabel, spend, pct, phrase, ratio, stackLabel } from '../format.js';

export type AttemptTab = 'checks' | 'screenshots' | 'files' | 'log' | 'transcript';

export interface AttemptPageInput {
  sheet: CampaignSheet;
  attemptId: string;
  tab: AttemptTab;
  checks: AttemptChecks | null;
  evidence: AttemptPackage | null;
  log: string;
  transcript?: TranscriptPage | null;
  timeBudget?: ReturnType<typeof readCampaignTimeBudget> | null;
  canControl?: boolean;
  controlError?: string;
}

const GLYPH: Record<string, string> = { pass: '<span class="p">✓</span>',
  fail: '<span class="f">✕</span>', 'not-run': '<span class="x">·</span>' };

function locate(sheet: CampaignSheet, attemptId: string): {
  stack: SheetStack;
  attempt: SheetAttempt;
} | null {
  for (const stack of sheet.stacks) {
    const attempt = stack.attempts.find(item => item.id === attemptId);
    if (attempt) return { stack, attempt };
  }
  return null;
}

function checksTable(checks: AttemptChecks | null): string {
  if (!checks?.checks.length) return '<p class="summary-note">No check results are recorded yet. Check the log for current work or an execution error.</p>';
  const features = new Map<string, AttemptCheck[]>();
  for (const check of checks.checks) {
    features.set(check.feature, [...features.get(check.feature) ?? [], check]);
  }
  const groups = [...features.entries()].map(([feature, items]) => {
    const points = items.reduce((total, check) => total + check.points, 0);
    const passed = items.filter(check => check.outcome === 'pass')
      .reduce((total, check) => total + check.points, 0);
    return `<tr class="group"><td colspan="3">${esc(feature)}`
      + `<i>${ratio(passed, points)}</i></td></tr>`
      + items.map(check => `<tr><td class="k">${esc(check.id)}</td>`
        + `<td class="d">${esc(check.description)}</td><td class="h">`
        + `${check.history.map(outcome => GLYPH[outcome] ?? GLYPH['not-run']).join('')}`
        + '</td></tr>').join('');
  }).join('');
  return '<div class="grade-key">Raw check outcomes from each grade, from left to right. '
    + '<span class="p">✓ Pass</span><span class="f">✕ Fail</span>'
    + '<span class="x">· No pass/fail result</span></div>'
    + '<div class="wrap"><table class="checks"><thead><tr><th>Check</th><th>Proves</th>'
    + `<th>Grades</th></tr></thead><tbody>${groups}</tbody></table></div>`;
}

function artifacts(evidence: AttemptPackage | null, key: string, visual: boolean): string {
  const items = (evidence?.executions ?? []).flatMap(execution =>
    visual ? execution.visuals : execution.artifacts.filter(item => item.kind !== 'visual'));
  const link = (id: string): string =>
    `/api/campaigns/${encodeURIComponent(key)}/artifacts/${encodeURIComponent(id)}`;
  if (!items.length) return `<p class="summary-note">No ${visual ? 'screenshots' : 'files'} are available for this attempt.</p>`;
  if (visual) {
    return `<div class="shots">${items.map(item => {
      const source = link(item.id);
      return `<button type="button" data-shot="${source}" data-shot-name="${esc(item.name)}">`
        + `<img src="${source}" alt="${esc(item.name)}"></button>`;
    }).join('')}</div><dialog class="lightbox"><form method="dialog">`
      + '<button type="submit">Close</button></form><img alt=""></dialog>';
  }
  return `<div class="files-list">${items.map(item =>
    `<a href="${link(item.id)}">${esc(item.path)}</a>`).join('')}</div>`;
}

export function attemptPage({ sheet, attemptId, tab, checks, evidence, log, transcript,
  timeBudget, canControl = false, controlError = '' }: AttemptPageInput): string {
  const found = locate(sheet, attemptId);
  const crumbs = (tail: string): string => `<div class="crumbs"><a href="/">Campaigns</a> / `
    + `<a href="/c/${encodeURIComponent(sheet.key)}">${esc(sheet.title)}</a> / `
    + `<b>${esc(tail)}</b></div>`;
  if (!found) {
    return `<div class="page">${crumbs(attemptId)}`
      + '<div class="title"><h2>Attempt not found</h2></div></div>';
  }
  const { stack, attempt } = found;
  const clock = timeBudget?.observedAt
    ? executionClock(new Date(Date.parse(timeBudget.observedAt) - timeBudget.consumedMs).toISOString(),
      attempt.status === 'running' ? null : timeBudget.observedAt)
    : executionClock(attempt.executionStartedAt, attempt.executionCompletedAt);
  const latestGrant = timeBudget?.grants.slice().sort((a, b) =>
    a.request.requestedAt.localeCompare(b.request.requestedAt)).at(-1);
  const pending = timeBudget?.grants.some(grant => grant.disposition === 'pending') ?? false;
  const resumeWithTime = attempt.status !== 'running' && timeBudget?.continuation?.eligible === true;
  const timeControls = canControl && timeBudget && (resumeWithTime || (attempt.status === 'running' && timeBudget.liveGrantSupported))
    ? `<form class="time-grant secret" data-run="grant-time" data-resume="${resumeWithTime}">`
      + '<label>Add minutes <input name="minutes" type="number" min="1" step="1" value="120" required></label>'
      + '<input name="secret" type="password" aria-label="Operator secret" placeholder="Operator secret" autocomplete="off" required>'
      + `<button class="btn" type="submit"${pending ? ' disabled' : ''}>${pending ? 'Awaiting controller' : resumeWithTime ? 'Add time and resume' : 'Add time'}</button>`
      + `<output data-time-base="${timeBudget.effectiveMinutes}">Limit after request: ${duration((timeBudget.effectiveMinutes + 120) * 60)}</output>`
      + `<span class="summary-note">${resumeWithTime ? 'Continues from the verified checkpoint.' : 'Keeps the agent running.'} Cost and repair limits stay fixed.</span></form>`
    : canControl && attempt.status === 'running' && timeBudget?.liveGrantSupported === false
      ? '<p class="summary-note">This controller does not support live time extensions.</p>'
      : canControl && timeBudget?.continuation?.reason
        ? `<p class="summary-note">Cannot resume: ${esc(timeBudget.continuation.reason)}</p>` : '';
  const grantStatus = latestGrant ? `<p class="summary-note" role="status">${esc(
    latestGrant.disposition === 'pending' ? 'Time request pending. The limit has not changed yet.'
      : latestGrant.disposition === 'accepted' ? `Time added. Limit: ${duration(timeBudget!.effectiveMinutes * 60)}.`
        : `Time request rejected: ${latestGrant.reason ?? 'See the grant evidence.'}`)}</p>` : '';
  const name = `${stackLabel(stack.stack)} rep ${attempt.repetition}`;
  const counts: Record<AttemptTab, string> = {
    checks: checks ? String(checks.checks.length) : '',
    screenshots: evidence
      ? String(evidence.executions.reduce((total, item) => total + item.visuals.length, 0)) : '',
    files: evidence ? String(evidence.executions.reduce((total, item) =>
      total + item.artifacts.filter(entry => entry.kind !== 'visual').length, 0)) : '',
    transcript: attempt.status === 'running' ? 'live' : '',
    log: attempt.status === 'running' ? 'live' : '',
  };
  const tabs = (['checks', 'transcript', 'screenshots', 'files', 'log'] as const).map(entry =>
    `<a class="${entry === tab ? 'on' : ''}"${entry === tab ? ' aria-current="page"' : ''} href="?tab=${entry}">`
    + `${entry[0]!.toUpperCase()}${entry.slice(1)}`
    + `${counts[entry] ? `<i>${esc(counts[entry])}</i>` : ''}</a>`).join('');
  const help: Record<string, string> = {
    Completion: 'Accepted checks passed out of every selected check, including checks not reached.',
    'Weighted score': 'Points earned across the selected grading scope. This differs from the number of checks passed.',
    'Before repairs': 'Score from the first build at each level, before repairs at that level. Later levels retain earlier fixes and feedback. A dash means no usable first-build evidence.',
    Repairs: 'Completed repairs out of the planned allowance for this attempt. Per-feature limits still apply.',
    Elapsed: 'Consumed time across executions / effective time limit. Includes coding, grading, repairs, and host sleep. Time between executions is excluded.'
      + (timeBudget ? ` Original limit: ${duration(timeBudget.originalMinutes * 60)}. Accepted extensions: ${timeBudget.extensionCount}.` : ''),
    Time: 'Recorded attempt duration. A dash means duration evidence is not yet available.',
    Spend: 'Cost from saved receipts and the pinned price snapshot. Work since the last checkpoint can include an unfinished depth and is not yet counted. Unknown is not zero; an upper bound starts with an inequality sign.',
  };
  const figure = (label: string, text: string, tone = ''): string =>
    `<div><div class="metric-label">${metricLabel(label, help[label])}</div><b class="${tone}">${text}</b></div>`;
  const stage = (level: number): string =>
    sheet.mode === 'dependency' ? `depth ${level}` : `L${level}`;
  const panel = tab === 'transcript' ? transcriptPanel(transcript)
    : tab === 'checks' ? checksTable(checks)
    : tab === 'log' ? (log ? `<pre class="log">${esc(log)}</pre>` : '<p class="summary-note">No log output is recorded yet.</p>')
      : artifacts(evidence, sheet.key, tab === 'screenshots');
  const issue = attempt.excluded
    ? `<div class="issue"><span class="label">Why this run was excluded</span>`
      + `<p>${esc(attempt.excluded)}</p></div>` : '';
  const dependency = sheet.mode === 'dependency'
    ? '<p class="grade-key">Completion uses the full selected target. Blocked descendants receive no credit, even if a raw check passed. '
      + (attempt.completion ? `${attempt.completion.unmeasured} checks have no accepted outcome. `
        : 'The count without an accepted outcome is unavailable. ')
      + 'This can mean a guarantee was deferred by a prerequisite or conclusive evidence is missing. '
      + 'The saved summary does not separate these causes. The Checks tab shows raw grade outcomes, not accepted completion.</p>' : '';
  return `<div class="page">${crumbs(name)}`
    + `<div class="title"><h2>${esc(stackLabel(stack.stack))} `
    + `<span>rep ${attempt.repetition}</span></h2></div>`
    + (sheet.provisional ? '<p class="summary-note">Provisional results: qualification is incomplete.</p>' : '')
    + `<div class="figs">${figure('Completion', attempt.completion ? ratio(attempt.completion.passed, attempt.completion.selected) : DASH)}`
    + figure('Spend', spend(attempt.spend, attempt.spendPending))
    + figure('Status', esc(phrase(attempt)), attempt.stalling ? 'now warn' : 'now')
    + figure('Weighted score', pct(attempt.score))
    + figure('Before repairs', pct(attempt.unaided))
    + figure('Repairs', ratio(attempt.repairs.used, attempt.repairs.budget))
    + figure('Elapsed', attempt.status === 'running' || attempt.executionCompletedAt
      ? clock
        + ` / ${duration((timeBudget?.effectiveMinutes ?? sheet.facts.timeLimitMinutes) * 60)}` : DASH)
    + figure('Time', duration(attempt.timeSec))
    + `</div>${timeControls}${grantStatus}${controlError ? `<p class="err" role="alert">${esc(controlError)}</p>` : ''}${issue}${dependency}<h3>Grade history</h3>${bigClimb(attempt.climb, stage)}`
    + `<div class="tabs">${tabs}</div>${panel}</div>`;
}

function transcriptPanel(page?: TranscriptPage | null): string {
  if (!page) return '<div class="loading" role="status">Loading transcript�</div>';
  if (!page.sessions.length) return '<p class="summary-note">No transcript is available for this run yet.</p>';
  return '<div class="transcript-controls"><label>Session <select data-transcript-session aria-label="Transcript session">'
    + page.sessions.map(session => `<option value="${esc(session.id)}"${session.id === page.session ? ' selected' : ''}>${esc(session.label)}</option>`).join('')
    + '</select></label>'
    + (page.before === null ? '' : `<button data-transcript-before="${page.before}">Earlier</button>`)
    + '<button data-transcript-latest title="Show the newest messages. Live updates pause while you read earlier messages.">Latest</button></div>'
    + (page.skipped ? '<p class="summary-note">Some malformed transcript records could not be displayed.</p>' : '')
    + '<div class="transcript" tabindex="0" aria-label="Agent transcript">'
    + page.messages.map(message => message.tool
      ? `<details data-key="${esc(message.id)}"><summary>${esc(message.role)}</summary><pre>${esc(message.text)}</pre></details>`
      : `<article data-key="${esc(message.id)}"><strong>${esc(message.role)}</strong><pre>${esc(message.text)}</pre></article>`).join('')
    + '</div>';
}
