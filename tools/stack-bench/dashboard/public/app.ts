/// <reference lib="dom" />
/// <reference lib="dom.iterable" />

// The client: real paths, one event stream, and keyed reconciliation so a
// refresh does not move what the pointer is on. Every view is a pure function
// of data; the only DOM work in the dashboard happens here.

import type { AttemptChecks, AttemptPackage, CampaignProgression, CampaignSheet, OverviewEntry }
  from '../dashboard-views.js';
import type { DashboardPlan } from '../dashboard-model.js';
import type { readCampaignTimeBudget } from '../../src/campaigns/campaign-time-grant.js';
import { type QuestlineView, campaignPage, replayTimeline, selectedProgression } from './views/campaign.js';
import { type AttemptTab, attemptPage } from './views/attempt.js';
import { type CampaignFilter, campaignsPage } from './views/campaigns.js';
import { type Page, type RunForm, afterRun, plansPage, runName, topbar }
  from './views/plans.js';
import { duration, elapsed, esc } from './format.js';

const FALLBACK_MS = 15_000;
const TABS: readonly AttemptTab[] = ['checks', 'screenshots', 'files', 'log'];
const VIEWS: readonly QuestlineView[] = ['grid', 'graph', 'replay'];
const FILTERS: readonly CampaignFilter[] = ['all', 'attention', 'completed', 'ready'];

interface Route {
  key: string;
  attempt: string;
  plans: boolean;
  filter: CampaignFilter;
  view: QuestlineView;
  step: number;
  tab: AttemptTab;
}

const state = {
  overview: [] as OverviewEntry[],
  plans: [] as DashboardPlan[],
  canStart: false,
  csrfToken: '',
  readError: '',
  form: { planId: '', outputName: '', secret: '', error: '' } as RunForm,
  sheets: new Map<string, CampaignSheet>(),
  progression: new Map<string, CampaignProgression | null>(),
  checks: new Map<string, AttemptChecks>(),
  evidence: new Map<string, AttemptPackage>(),
  timeBudgets: new Map<string, ReturnType<typeof readCampaignTimeBudget>>(),
  timeGrantIds: new Map<string, string>(),
  timeGrantMinutes: '120',
  log: { attempt: '', text: '', offset: 0 },
};
let fallback = 0;
let playing = 0;
let submitting = false;
let loading = false;
let loadVersion = 0;

function route(): Route {
  const url = new URL(location.href);
  const parts = url.pathname.split('/').filter(Boolean);
  const pick = <Value extends string>(values: readonly Value[], name: string, fall: Value): Value =>
    values.find(value => value === url.searchParams.get(name)) ?? fall;
  return {
    key: parts[0] === 'c' ? parts[1] ?? '' : '',
    attempt: parts[2] === 'a' ? parts[3] ?? '' : '',
    plans: parts[0] === 'plans',
    filter: pick(FILTERS, 'filter', 'all'),
    view: pick(VIEWS, 'questlines', 'grid'),
    step: Math.max(0, Number(url.searchParams.get('step') ?? 0)),
    tab: pick(TABS, 'tab', 'checks'),
  };
}

async function read<Payload>(url: string): Promise<Payload | null> {
  const version = loadVersion;
  try {
    const response = await fetch(url, { headers: { accept: 'application/json' }, signal: AbortSignal.timeout(30_000) });
    if (!response.ok) {
      const failure = await response.json().catch(() => ({})) as { error?: string };
      if (version === loadVersion) state.readError = failure.error ?? `Request failed (${response.status}).`;
      return null;
    }
    const payload = await response.json() as Payload;
    return version === loadVersion ? payload : null;
  } catch {
    if (version === loadVersion) state.readError = 'The dashboard did not respond. Check its connection and try again.';
    return null;
  }
}

function attemptUrl(current: Route, suffix: string): string {
  return `/api/campaigns/${encodeURIComponent(current.key)}`
    + `/attempts/${encodeURIComponent(current.attempt)}/${suffix}`;
}

async function readLog(current: Route): Promise<void> {
  if (state.log.attempt !== current.attempt) state.log = { attempt: current.attempt, text: '', offset: 0 };
  try {
    const response = await fetch(attemptUrl(current, `log?from=${state.log.offset}`), { signal: AbortSignal.timeout(30_000) });
    if (!response.ok) throw new Error('Log request failed');
    const text = await response.text();
    if (state.log.attempt !== current.attempt) return;
    state.log.text += text;
    state.log.offset = Number(response.headers.get('x-stack-bench-log-offset') ?? state.log.offset);
  } catch {
    if (route().attempt === current.attempt) state.readError = 'Could not load the run log. Try again.';
  }
}

function chrome(current: Route): string {
  const sheet = state.sheets.get(current.key) ?? null;
  const page: Page = current.plans ? 'plans'
    : current.key && !current.attempt ? 'campaign' : 'campaigns';
  return topbar({ page, key: current.key, canStart: state.canStart, error: state.form.error,
    controllerOwner: page === 'campaign' ? sheet?.controllerOwner : null,
    resumable: state.canStart && page === 'campaign' && (sheet?.resumable ?? false) });
}

function page(current: Route): string {
  const sheet = state.sheets.get(current.key) ?? null;
  if (current.plans) {
    return plansPage({ plans: state.plans, canStart: state.canStart, form: state.form });
  }
  if (!current.key) {
    const running = state.overview.filter(campaign => campaign.status === 'running')
      .map(campaign => state.sheets.get(campaign.key))
      .filter((entry): entry is CampaignSheet => entry !== undefined);
    return campaignsPage({ campaigns: state.overview, sheets: running, filter: current.filter });
  }
  if (!sheet) return `<div class="page"><div class="crumbs"><a href="/">Campaigns</a> / `
    + `<b>${esc(current.key)}</b></div></div>`;
  if (current.attempt) {
    return attemptPage({ sheet, attemptId: current.attempt, tab: current.tab,
      timeBudget: state.timeBudgets.get(current.attempt), canControl: state.canStart,
      controlError: state.form.error,
      checks: state.checks.get(current.attempt) ?? null,
      evidence: state.evidence.get(current.attempt) ?? null,
      log: state.log.attempt === current.attempt ? state.log.text : '' });
  }
  return campaignPage({ sheet, progression: state.progression.get(current.key) ?? null,
    view: current.view, step: current.step });
}

function sync(current: Element, next: Element): void {
  for (const name of [...current.getAttributeNames()]) {
    if (current.tagName === 'DETAILS' && name === 'open') continue;
    if (!next.hasAttribute(name)) current.removeAttribute(name);
  }
  for (const name of next.getAttributeNames()) {
    if (current.getAttribute(name) !== next.getAttribute(name)) {
      current.setAttribute(name, next.getAttribute(name) ?? '');
    }
  }
}

// Replace only what changed, matching children by position and data-key, so a
// row under the pointer keeps its hover across a refetch.
function patch(current: Element, next: Element): void {
  const mine = [...current.children];
  const theirs = [...next.children];
  if (mine.length !== theirs.length || current.childNodes.length !== mine.length
    || next.childNodes.length !== theirs.length) {
    current.replaceChildren(...next.childNodes);
    return;
  }
  mine.forEach((child, index) => {
    const other = theirs[index]!;
    if (child.tagName !== other.tagName
      || child.getAttribute('data-key') !== other.getAttribute('data-key')) {
      child.replaceWith(other);
      return;
    }
    if (child.outerHTML === other.outerHTML) return;
    if (!child.children.length || !other.children.length) {
      child.replaceWith(other);
      return;
    }
    sync(child, other);
    patch(child, other);
  });
}

function updateTimeTotal(field: HTMLInputElement): void {
  const total = field.form?.querySelector<HTMLOutputElement>('[data-time-base]');
  if (total) total.textContent = field.validity.valid
    ? `Limit after request: ${duration((Number(total.dataset.timeBase) + field.valueAsNumber) * 60)}`
    : 'Enter positive whole minutes.';
}

function render(): void {
  const current = route();
  const root = document.body;
  const next = document.createElement('body');
  next.innerHTML = `${chrome(current)}<main aria-busy="${loading}">`
    + (state.readError ? `<div class="page err" role="alert">${esc(state.readError)} <button type="button" data-retry>Retry</button></div>` : '')
    + (loading ? `<div class="page loading" role="status">Loading ${current.plans ? 'plans' : current.attempt ? 'run details' : current.key ? 'campaign' : 'campaigns'}…</div>` : page(current))
    + '</main>';
  patch(root, next);
  // The secret and the run name live in the tab, never in the markup.
  for (const field of document.querySelectorAll<HTMLInputElement>('form[data-run] input')) {
    if (field.name === 'minutes') {
      field.value = state.timeGrantMinutes;
      updateTimeTotal(field);
    }
    if (field.name !== 'secret' && field.name !== 'output') continue;
    const value = field.name === 'secret' ? state.form.secret : state.form.outputName;
    if (field.value !== value) field.value = value;
  }
  for (const form of document.querySelectorAll<HTMLFormElement>('form[data-run]')) {
    form.setAttribute('aria-busy', String(submitting));
    for (const button of form.querySelectorAll<HTMLButtonElement>('button[type=submit]')) {
      button.disabled = submitting || (form.dataset.run === 'grant-time'
        && (state.timeBudgets.get(current.attempt)?.grants.some(grant => grant.disposition === 'pending') ?? false));
    }
  }
}

async function load(navigation = false): Promise<void> {
  if (loading && !navigation) return;
  const version = ++loadVersion;
  loading = navigation;
  state.readError = '';
  if (navigation) render();
  try {
    await loadData(version);
  } catch {
    if (version === loadVersion) state.readError = 'Could not load this page. Try again.';
  } finally {
    if (version === loadVersion) {
      loading = false;
      render();
    }
  }
}

async function loadData(version: number): Promise<void> {
  const current = route();
  if (!current.key || !state.csrfToken) {
    const overview = await read<{ campaigns: OverviewEntry[]; canStart: boolean;
      csrfToken: string; }>('/api/overview');
    if (version !== loadVersion) return;
    if (overview) Object.assign(state, { overview: overview.campaigns,
      canStart: overview.canStart, csrfToken: overview.csrfToken });
    render();
  }
  if (current.plans) {
    const plans = await read<DashboardPlan[]>('/api/plans');
    if (version !== loadVersion) return;
    if (plans) state.plans = plans;
    const first = state.plans.find(plan => plan.state === 'frozen');
    if (first && !state.form.planId) {
      state.form = { ...state.form, planId: first.id, outputName: runName(first.id, new Date()) };
    }
    render();
    return;
  }
  if (!current.key) {
    for (const campaign of state.overview.filter(entry => entry.status === 'running')) {
      const sheet = await read<CampaignSheet>(`/api/campaigns/${encodeURIComponent(campaign.key)}`);
      if (version !== loadVersion) return;
      if (sheet) state.sheets.set(campaign.key, sheet);
      render();
    }
    return;
  }
  const sheet = await read<CampaignSheet>(`/api/campaigns/${encodeURIComponent(current.key)}`);
  if (version !== loadVersion) return;
  if (sheet) state.sheets.set(current.key, sheet);
  render();
  if (sheet?.mode === 'dependency' && current.view !== 'grid'
    && !state.progression.has(current.key)) {
    const progression = await read<CampaignProgression>(
      `/api/campaigns/${encodeURIComponent(current.key)}/progression`);
    if (version !== loadVersion) return;
    if (progression) state.progression.set(current.key, progression);
    render();
  }
  if (!current.attempt) return;
  const timeBudget = await read<ReturnType<typeof readCampaignTimeBudget>>(attemptUrl(current, 'time'));
  if (version !== loadVersion) return;
  if (timeBudget) {
    state.timeBudgets.set(current.attempt, timeBudget);
    if (timeBudget.grants.some(grant => grant.request.grantId === state.timeGrantIds.get(current.attempt)
      && grant.disposition !== 'pending')) state.timeGrantIds.delete(current.attempt);
  }
  if (current.tab === 'checks' && !state.checks.has(current.attempt)) {
    const checks = await read<AttemptChecks>(attemptUrl(current, 'checks'));
    if (checks) state.checks.set(current.attempt, checks);
  } else if ((current.tab === 'screenshots' || current.tab === 'files')
    && !state.evidence.has(current.attempt)) {
    const evidence = await read<AttemptPackage>(attemptUrl(current, 'package'));
    if (evidence) state.evidence.set(current.attempt, evidence);
  } else if (current.tab === 'log') {
    await readLog(current);
  }
  render();
}

function go(href: string): void {
  history.pushState(null, '', href);
  void load(true);
}

function stepTo(offset: number): void {
  const current = route();
  const progression = state.progression.get(current.key) ?? null;
  const sheet = state.sheets.get(current.key);
  if (!progression || !sheet) return;
  const total = replayTimeline(selectedProgression(progression, sheet)).length;
  const next = Math.min(Math.max(0, current.step + offset), Math.max(0, total - 1));
  const url = new URL(location.href);
  url.searchParams.set('step', String(next));
  history.replaceState(null, '', `${url.pathname}${url.search}`);
  render();
}

function subscribe(): void {
  const source = new EventSource('/api/events');
  const changed = (event: MessageEvent<string>): void => {
    const current = route();
    const message = JSON.parse(event.data) as { key?: string; attemptId?: string };
    if (current.key && message.key !== current.key) return;
    if (message.attemptId && message.attemptId !== current.attempt) return;
    if (message.attemptId) state.checks.delete(message.attemptId);
    void load();
  };
  source.addEventListener('campaign', changed);
  source.addEventListener('log', changed);
  source.addEventListener('open', () => {
    if (fallback) clearInterval(fallback);
    fallback = 0;
  });
  // Only while the stream is down: a served dashboard that is up pays nothing.
  source.addEventListener('error', () => {
    fallback ||= window.setInterval(() => void load(), FALLBACK_MS);
  });
}

let helpClose = 0;
for (const type of ['pointerover', 'focusin']) document.addEventListener(type, event => {
  if (!(event.target instanceof Element)) return;
  if (!event.target.closest('.metric-help, .metric-tooltip')) return;
  clearTimeout(helpClose);
  const trigger = event.target.closest<HTMLButtonElement>('.metric-help');
  trigger?.click();
});
for (const type of ['pointerout', 'focusout']) document.addEventListener(type, event => {
  if (!(event.target instanceof Element)
    || !event.target.closest('.metric-help, .metric-tooltip')) return;
  clearTimeout(helpClose);
  helpClose = window.setTimeout(() => {
    if (document.querySelector('.metric-help:hover, .metric-help:focus, .metric-tooltip:hover')) return;
    document.querySelector<HTMLElement>('.metric-tooltip:popover-open')?.hidePopover();
  }, 150);
});

document.addEventListener('click', event => {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey
    || event.shiftKey || event.altKey) return;
  if ((event.target as Element | null)?.closest('[data-retry]')) {
    void load(true);
    return;
  }
  const shot = (event.target as Element | null)?.closest<HTMLElement>('[data-shot]');
  if (shot) {
    const dialog = document.querySelector<HTMLDialogElement>('.lightbox');
    const image = dialog?.querySelector('img');
    if (!dialog || !image) return;
    image.src = shot.dataset.shot ?? '';
    image.alt = shot.dataset.shotName ?? '';
    dialog.showModal();
    return;
  }
  if (event.target instanceof HTMLDialogElement) event.target.close();
  const link = (event.target as Element | null)?.closest('a');
  const href = link?.getAttribute('href') ?? '';
  if (!href || href.startsWith('/api/') || !/^[/?]/.test(href)) return;
  event.preventDefault();
  go(href.startsWith('?') ? `${location.pathname}${href}` : href);
});

// Start and resume are the same request twice: the browser token, the operator
// secret the operator just typed, and the plan the server re-reads itself.
async function post(form: HTMLFormElement): Promise<void> {
  if (submitting) return;
  const current = route();
  const data = new FormData(form);
  const action = form.dataset.run;
  const resumeWithTime = action === 'grant-time' && form.dataset.resume === 'true';
  const existing = action === 'resume' || action === 'stop' || action === 'grant-time';
  const output = existing ? current.key : String(data.get('output') ?? '');
  // Retain the ID after an uncertain response, so retry cannot add time twice.
  if (action === 'grant-time' && !state.timeGrantIds.has(current.attempt)) {
    state.timeGrantIds.set(current.attempt, crypto.randomUUID());
  }
  const grantId = state.timeGrantIds.get(current.attempt);
  submitting = true;
  render();
  let timeAccepted = false;
  try {
    let response = await fetch(action === 'grant-time' ? attemptUrl(current, 'time') : existing
      ? `/api/campaigns/${encodeURIComponent(current.key)}/${action}` : '/api/campaigns', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-stack-bench-token': state.csrfToken,
        'x-stack-bench-control-secret': String(data.get('secret') ?? '') },
      body: JSON.stringify(action === 'grant-time' ? { grantId, minutes: Number(data.get('minutes')) }
        : action === 'stop' ? { owner: data.get('owner') }
        : existing ? {} : { planId: String(data.get('plan') ?? ''), outputName: output }),
    });
    if (response.ok && resumeWithTime) {
      timeAccepted = true;
      response = await fetch(`/api/campaigns/${encodeURIComponent(current.key)}/resume`, {
        method: 'POST', headers: { 'content-type': 'application/json', 'x-stack-bench-token': state.csrfToken,
          'x-stack-bench-control-secret': String(data.get('secret') ?? '') }, body: '{}',
      });
    }
    if (response.ok) {
      state.form = { ...state.form, secret: '', error: '' };
      if (existing) return void load();
      return go(`/c/${encodeURIComponent(output)}`);
    }
    const failure = await response.json().catch(() => ({})) as { error?: string };
    state.form = afterRun(state.form, response.status,
      (timeAccepted ? 'Time was added, but resume failed. ' : '')
      + (failure.error || `Request failed (HTTP ${response.status}). Check campaign status before retrying.`));
  } catch {
    state.form = { ...state.form,
      error: (timeAccepted ? 'Time was added. Could not confirm resume. ' : 'Could not confirm the request. ')
        + 'Check campaign status before retrying.' };
  } finally {
    submitting = false;
    render();
  }
}

document.addEventListener('submit', event => {
  const form = event.target;
  if (!(form instanceof HTMLFormElement) || !form.dataset.run) return;
  event.preventDefault();
  void post(form);
});

// The form's fields are the state; picking a plan renames the output with it.
document.addEventListener('input', event => {
  const field = event.target as HTMLInputElement;
  if (field.name === 'minutes') {
    state.timeGrantMinutes = field.value;
    updateTimeTotal(field);
  }
  if (field.name === 'secret') state.form = { ...state.form, secret: field.value };
  else if (field.name === 'output') state.form = { ...state.form, outputName: field.value };
  else if (field.name === 'plan') {
    state.form = { ...state.form, planId: field.value,
      outputName: runName(field.value, new Date()) };
    render();
  }
});

document.addEventListener('keydown', event => {
  if (route().view !== 'replay') return;
  if (event.target instanceof Element
    && event.target.closest('input, textarea, select, button, summary, [contenteditable]')) return;
  if (event.key === 'ArrowRight') stepTo(1);
  else if (event.key === 'ArrowLeft') stepTo(-1);
  else if (event.key === ' ') {
    event.preventDefault();
    if (playing) {
      clearInterval(playing);
      playing = 0;
    } else playing = window.setInterval(() => stepTo(1), 600);
    return;
  } else return;
  if (playing) {
    clearInterval(playing);
    playing = 0;
  }
});

window.setInterval(() => {
  for (const clock of document.querySelectorAll<HTMLElement>('[data-started-at]')) {
    clock.textContent = elapsed(clock.dataset.startedAt ?? null, null);
  }
}, 1000);
window.addEventListener('popstate', () => void load(true));
subscribe();
void load(true);
