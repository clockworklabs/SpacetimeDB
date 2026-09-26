// Saved plan summaries and shared navigation. New runs use the setup page.

import type { DashboardPlan } from '../../dashboard-model.js';
import { DASH, duration, esc, money, num } from '../format.js';

export type Page = 'campaigns' | 'plans' | 'campaign' | 'check-guide';

export interface RunForm {
  error: string;
}

const HEADS: Array<[string, string]> = [['Plan', 'name'], ['Mode', 'shape'], ['Shape', 'shape'],
  ['Stacks', 'stack'], ['Attempts', 'stack'], ['Parallel', 'stack'], ['Repairs', 'stack'],
  ['Time limit', 'stack'], ['Attempt cap', 'stack'], ['Campaign cap', 'stack'], ['State', 'state']];

export function runName(planId: string, now: Date): string {
  const pad = (value: number): string => String(value).padStart(2, '0');
  const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`
    + `-${pad(now.getHours())}${pad(now.getMinutes())}`;
  return `${planId}-${stamp}`.toLowerCase().replace(/[^a-z0-9.-]+/g, '-')
    .replace(/^[^a-z0-9]+/, '').slice(0, 120);
}

export function topbar({ page, key, canStart, resumable, controllerOwner, error, reportFiles = [] }: {
  page: Page; key: string; canStart: boolean; resumable: boolean; controllerOwner?: string | null; error: string;
  reportFiles?: string[];
}): string {
  const artifact = (path: string): string => `/api/campaigns/${encodeURIComponent(key)}/artifacts/`
    + btoa(path).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const files = page === 'campaign'
    ? '<details class="files"><summary class="btn">Files</summary><div>'
      + `<a href="${artifact('plan.json')}">plan</a><a href="${artifact('state.json')}">state</a>`
      + (reportFiles.includes('report/report.html') ? `<a href="${artifact('report/report.html')}">report</a>` : '')
      + (reportFiles.includes('report/export-manifest.json') ? `<a href="${artifact('report/export-manifest.json')}">export manifest</a>` : '')
      + '</div></details>' : '';
  const resume = resumable
    ? '<form class="secret" data-run="resume">'
      + '<button class="btn" type="submit">Resume</button>'
      + (error ? `<span class="err">${esc(error)}</span>` : '') + '</form>' : '';
  const stop = canStart && controllerOwner
    ? `<form class="secret" data-run="stop"><input type="hidden" name="owner" value="${esc(controllerOwner)}">`
      + '<button class="btn" type="submit">Stop</button>'
      + (error ? `<span class="err">${esc(error)}</span>` : '') + '</form>' : '';
  const nav = (on: boolean, label: string, href: string): string =>
    `<a class="${on ? 'on' : ''}" href="${href}">${label}</a>`;
  return '<div class="topbar"><a class="brand" href="/">'
    + '<img src="/spacetimedb-mark.svg" alt="" width="26" height="24"><b>STACK BENCH</b></a>'
    + `<nav class="nav">${nav(page === 'campaigns' || page === 'campaign', 'Campaigns', '/')}`
    + `${nav(page === 'check-guide', 'Checks', '/checks')}</nav><div class="tools">${stop}${resume}${files}`
    + '<a class="btn primary" href="/new">New run</a></div></div>';
}

function shapeOf(plan: DashboardPlan): string {
  const levels = plan.levels ?? [];
  if (!levels.length) return DASH;
  const depth = Math.max(...levels);
  if (plan.mode === 'dependency') return `depth ${depth}`;
  return levels.length > 1 ? `L${Math.min(...levels)}–L${depth}` : `L${depth}`;
}

function planRow(plan: DashboardPlan): string {
  const budgets = plan.budgets ?? null;
  const stacks = plan.stacks ?? [];
  const cell = (value: string, hover = ''): string =>
    `<td class="stack" title="${esc(hover || value)}">${value}</td>`;
  return `<tr data-key="${esc(plan.file)}">`
    + `<td class="name" title="${esc(plan.file)}">${esc(plan.title)}</td>`
    + `<td class="shape">${esc(plan.mode ?? DASH)}</td>`
    + `<td class="shape">${esc(shapeOf(plan))}</td>`
    + cell(stacks.length ? num(stacks.length) : DASH, stacks.join(' · '))
    + cell(num(plan.attempts)) + cell(num(plan.parallelism))
    + cell(plan.repairBudget === undefined ? DASH : num(plan.repairBudget))
    + cell(budgets ? duration(budgets.attemptTimeoutMinutes * 60) : DASH)
    + cell(budgets ? money(budgets.maxCostUsdPerAttempt) : DASH)
    + cell(budgets?.maxCostUsdPerAttempt != null && plan.attempts != null
      ? money(budgets.maxCostUsdPerAttempt * plan.attempts) : DASH,
      'Maximum across planned attempts; each attempt cap includes its retries')
    + `<td><span class="state ${plan.state === 'frozen' ? 'done' : 'idle'}" `
    + `title="${esc(plan.error ?? plan.state)}">${esc(plan.state)}</span></td></tr>`;
}

export function plansPage({ plans, loading = false }: {
  plans: readonly DashboardPlan[]; loading?: boolean;
}): string {
  return `<div class="page"><div class="title"><h2>Saved plans</h2></div>`
    + '<p class="summary-note">Plans record the exact configuration behind a run. Use New run to select its settings.</p>'
    + '<div class="tablewrap"><div class="wrap"><table class="runs plans"><thead><tr>'
    + HEADS.map(([label, kind]) => `<th class="${kind}">${label}</th>`).join('')
    + `</tr></thead><tbody>${loading
      ? `<tr><td colspan="${HEADS.length}"><div class="loading" role="status">Loading plans…</div></td></tr>`
      : plans.length ? plans.map(planRow).join('')
      : `<tr><td colspan="${HEADS.length}">No plans</td></tr>`}</tbody></table></div></div></div>`;
}
