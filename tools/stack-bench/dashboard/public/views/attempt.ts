// One attempt: figures, the climb at full size, and the evidence behind tabs.
// Each tab is a link, so what is open survives a reload and a back button.

import type { AttemptCheck, AttemptChecks, AttemptPackage, CampaignSheet, SheetAttempt, SheetStack }
  from '../../dashboard-views.js';
import { bigClimb } from '../climb.js';
import { DASH, duration, esc, money, pct, phrase, ratio, stackLabel } from '../format.js';

export type AttemptTab = 'checks' | 'screenshots' | 'files' | 'log';

export interface AttemptPageInput {
  sheet: CampaignSheet;
  attemptId: string;
  tab: AttemptTab;
  checks: AttemptChecks | null;
  evidence: AttemptPackage | null;
  log: string;
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
  if (!checks) return '';
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
  return '<div class="wrap"><table class="checks"><thead><tr><th>Check</th><th>Proves</th>'
    + `<th>Grades</th></tr></thead><tbody>${groups}</tbody></table></div>`;
}

function artifacts(evidence: AttemptPackage | null, key: string, visual: boolean): string {
  const items = (evidence?.executions ?? []).flatMap(execution =>
    visual ? execution.visuals : execution.artifacts.filter(item => item.kind !== 'visual'));
  const link = (id: string): string =>
    `/api/campaigns/${encodeURIComponent(key)}/artifacts/${encodeURIComponent(id)}`;
  if (visual) {
    return `<div class="shots">${items.map(item =>
      `<a href="${link(item.id)}"><img src="${link(item.id)}" alt="${esc(item.name)}"></a>`)
      .join('')}</div>`;
  }
  return `<div class="files-list">${items.map(item =>
    `<a href="${link(item.id)}">${esc(item.path)}</a>`).join('')}</div>`;
}

export function attemptPage({ sheet, attemptId, tab, checks, evidence, log }: AttemptPageInput): string {
  const found = locate(sheet, attemptId);
  const crumbs = (tail: string): string => `<div class="crumbs"><a href="/">Campaigns</a> / `
    + `<a href="/c/${encodeURIComponent(sheet.key)}">${esc(sheet.title)}</a> / `
    + `<b>${esc(tail)}</b></div>`;
  if (!found) {
    return `<div class="page">${crumbs(attemptId)}`
      + '<div class="title"><h2>no attempt</h2></div></div>';
  }
  const { stack, attempt } = found;
  const name = `${stackLabel(stack.stack)} rep ${attempt.repetition}`;
  const counts: Record<AttemptTab, string> = {
    checks: checks ? String(checks.checks.length) : '',
    screenshots: evidence
      ? String(evidence.executions.reduce((total, item) => total + item.visuals.length, 0)) : '',
    files: evidence ? String(evidence.executions.reduce((total, item) =>
      total + item.artifacts.filter(entry => entry.kind !== 'visual').length, 0)) : '',
    log: attempt.status === 'running' ? 'live' : '',
  };
  const tabs = (['checks', 'screenshots', 'files', 'log'] as const).map(entry =>
    `<a class="${entry === tab ? 'on' : ''}" href="?tab=${entry}">`
    + `${entry[0]!.toUpperCase()}${entry.slice(1)}`
    + `${counts[entry] ? `<i>${esc(counts[entry])}</i>` : ''}</a>`).join('');
  const figure = (label: string, text: string, tone = ''): string =>
    `<div><span class="label">${esc(label)}</span><b class="${tone}">${text}</b></div>`;
  const stage = (level: number): string =>
    sheet.mode === 'dependency' ? `depth ${level}` : `L${level}`;
  const panel = tab === 'checks' ? checksTable(checks)
    : tab === 'log' ? `<pre class="log">${esc(log)}</pre>`
      : artifacts(evidence, sheet.key, tab === 'screenshots');
  return `<div class="page">${crumbs(name)}`
    + `<div class="title"><h2>${esc(stackLabel(stack.stack))} `
    + `<span>rep ${attempt.repetition}</span></h2></div>`
    + `<div class="figs">${figure('Score', pct(attempt.score), sheet.provisional ? 'prov' : '')}`
    + figure('Unaided', pct(attempt.unaided))
    + figure('Repairs', ratio(attempt.repairs.used, attempt.repairs.budget))
    + figure('Time', duration(attempt.timeSec))
    + figure('Spend', money(attempt.spendUsd))
    + figure('Now', esc(phrase(attempt)), attempt.stalling ? 'now warn' : 'now')
    + `</div>${bigClimb(attempt.climb, stage) || `<p class="v">${DASH}</p>`}`
    + `<div class="tabs">${tabs}</div>${panel}</div>`;
}
