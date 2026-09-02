// Plans, and the chrome that carries the run controls: one table of test
// plans, the form that starts a run, and the topbar whose Start a run and
// Resume exist only where the server accepts them. Only a frozen plan runs.

import type { DashboardPlan } from '../../dashboard-model.js';
import { DASH, esc, money, num } from '../format.js';

export type Page = 'campaigns' | 'plans' | 'campaign';

export interface RunForm {
  planId: string;
  outputName: string;
  secret: string;
  error: string;
}

// The server's SAFE_NAME, spelled once here so the field cannot hold a name the
// route will reject.
export const RUN_NAME = '[a-z0-9][a-z0-9.-]{2,119}';

const HEADS: Array<[string, string]> = [['Plan', 'name'], ['Mode', 'shape'], ['Shape', 'shape'],
  ['Stacks', 'stack'], ['Attempts', 'stack'], ['Parallel', 'stack'], ['Repairs', 'stack'],
  ['Time limit', 'stack'], ['Spend limit', 'stack'], ['State', 'state']];

export function runName(planId: string, now: Date): string {
  const pad = (value: number): string => String(value).padStart(2, '0');
  const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`
    + `-${pad(now.getHours())}${pad(now.getMinutes())}`;
  return `${planId}-${stamp}`.toLowerCase().replace(/[^a-z0-9.-]+/g, '-')
    .replace(/^[^a-z0-9]+/, '').slice(0, 120);
}

// A 403 is the wrong operator secret: the tab forgets it and keeps the rest.
export function afterRun(form: RunForm, status: number, error: string): RunForm {
  return { ...form, secret: status === 403 ? '' : form.secret, error };
}

export function topbar({ page, key, canStart, resumable, error }: {
  page: Page; key: string; canStart: boolean; resumable: boolean; error: string;
}): string {
  const artifact = (path: string): string => `/api/campaigns/${encodeURIComponent(key)}/artifacts/`
    + btoa(path).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const files = page === 'campaign'
    ? '<details class="files"><summary class="btn">Files</summary><div>'
      + `<a href="${artifact('plan.json')}">plan</a><a href="${artifact('state.json')}">state</a>`
      + `<a href="${artifact('report/report.html')}">report</a></div></details>` : '';
  const resume = resumable
    ? '<form class="secret" data-run="resume"><input name="secret" type="password" required>'
      + '<button class="btn" type="submit">Resume</button>'
      + (error ? `<span class="err">${esc(error)}</span>` : '') + '</form>' : '';
  const nav = (on: boolean, label: string, href: string): string =>
    `<a class="${on ? 'on' : ''}" href="${href}">${label}</a>`;
  return '<div class="topbar"><a class="brand" href="/">'
    + '<img src="/spacetimedb-mark.svg" alt="" width="26" height="24"><b>STACK BENCH</b></a>'
    + `<nav class="nav">${nav(page !== 'plans', 'Campaigns', '/')}`
    + `${nav(page === 'plans', 'Plans', '/plans')}</nav><div class="tools">${resume}${files}`
    + `${canStart && page !== 'plans' ? '<a class="btn primary" href="/plans">Start a run</a>' : ''}`
    + '</div></div>';
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
    + cell(budgets ? num(budgets.fixRounds) : DASH)
    + cell(budgets ? `${budgets.attemptTimeoutMinutes} min` : DASH)
    + cell(budgets ? money(budgets.maxCostUsdPerAttempt) : DASH)
    + `<td><span class="state ${plan.state === 'frozen' ? 'done' : 'idle'}" `
    + `title="${esc(plan.error ?? plan.state)}">${esc(plan.state)}</span></td></tr>`;
}

function runForm(plans: readonly DashboardPlan[], form: RunForm): string {
  const frozen = plans.filter(plan => plan.state === 'frozen');
  const options = [...new Set(frozen.map(plan => plan.mode ?? 'sequential'))]
    .map(mode => `<optgroup label="${esc(mode)}">${frozen
      .filter(plan => (plan.mode ?? 'sequential') === mode)
      .map(plan => `<option value="${esc(plan.id)}"${plan.id === form.planId ? ' selected' : ''}>`
        + `${esc(plan.title)}</option>`).join('')}</optgroup>`).join('');
  const field = (label: string, control: string): string =>
    `<div><span class="label">${label}</span>${control}</div>`;
  return '<form class="runform" data-run="start">'
    + field('Plan', `<select name="plan" required>${options}</select>`)
    + field('Run name', `<input name="output" required pattern="${RUN_NAME}">`)
    + field('Secret', '<input name="secret" type="password" required>')
    + '<button class="btn primary" type="submit">Start a run</button>'
    + (form.error ? `<div class="err">${esc(form.error)}</div>` : '') + '</form>';
}

export function plansPage({ plans, canStart, form }: {
  plans: readonly DashboardPlan[]; canStart: boolean; form: RunForm;
}): string {
  return `<div class="page">${canStart ? runForm(plans, form) : ''}`
    + '<div class="tablewrap"><div class="wrap"><table class="runs plans"><thead><tr>'
    + HEADS.map(([label, kind]) => `<th class="${kind}">${label}</th>`).join('')
    + `</tr></thead><tbody>${plans.length ? plans.map(planRow).join('')
      : `<tr><td colspan="${HEADS.length}">no plans</td></tr>`}</tbody></table></div></div></div>`;
}
