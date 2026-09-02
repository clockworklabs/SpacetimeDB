/// <reference lib="dom" />
/// <reference lib="dom.iterable" />

// The client: real paths, one event stream, and keyed reconciliation so a
// refresh does not move what the pointer is on. Every view is a pure function
// of data; the only DOM work in the dashboard happens here.

import type { AttemptChecks, AttemptPackage, CampaignProgression, CampaignSheet, OverviewEntry }
  from '../dashboard-views.js';
import type { DashboardPlan } from '../dashboard-model.js';
import { type QuestlineView, campaignPage, replayTimeline } from './views/campaign.js';
import { type AttemptTab, attemptPage } from './views/attempt.js';
import { type CampaignFilter, campaignsPage } from './views/campaigns.js';
import { type Page, type RunForm, afterRun, plansPage, runName, topbar }
  from './views/plans.js';
import { esc } from './format.js';

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
  form: { planId: '', outputName: '', secret: '', error: '' } as RunForm,
  sheets: new Map<string, CampaignSheet>(),
  progression: new Map<string, CampaignProgression | null>(),
  checks: new Map<string, AttemptChecks>(),
  evidence: new Map<string, AttemptPackage>(),
  log: { attempt: '', text: '', offset: 0 },
};
let fallback = 0;
let playing = 0;

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
  try {
    const response = await fetch(url, { headers: { accept: 'application/json' } });
    if (!response.ok) return null;
    return await response.json() as Payload;
  } catch {
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
    const response = await fetch(attemptUrl(current, `log?from=${state.log.offset}`));
    if (!response.ok) return;
    state.log.text += await response.text();
    state.log.offset = Number(response.headers.get('x-stack-bench-log-offset') ?? state.log.offset);
  } catch { /* the stream reconnects and asks again */ }
}

function chrome(current: Route): string {
  const sheet = state.sheets.get(current.key) ?? null;
  const page: Page = current.plans ? 'plans'
    : current.key && !current.attempt ? 'campaign' : 'campaigns';
  return topbar({ page, key: current.key, canStart: state.canStart, error: state.form.error,
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
      checks: state.checks.get(current.attempt) ?? null,
      evidence: state.evidence.get(current.attempt) ?? null,
      log: state.log.attempt === current.attempt ? state.log.text : '' });
  }
  return campaignPage({ sheet, progression: state.progression.get(current.key) ?? null,
    view: current.view, step: current.step });
}

function sync(current: Element, next: Element): void {
  for (const name of [...current.getAttributeNames()]) {
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

function render(): void {
  const current = route();
  const root = document.body;
  const next = document.createElement('body');
  next.innerHTML = `${chrome(current)}<main>${page(current)}</main>`;
  patch(root, next);
  // The secret and the run name live in the tab, never in the markup.
  for (const field of document.querySelectorAll<HTMLInputElement>('form[data-run] input')) {
    const value = field.name === 'secret' ? state.form.secret : state.form.outputName;
    if (field.value !== value) field.value = value;
  }
}

async function load(): Promise<void> {
  const current = route();
  if (!current.key || !state.csrfToken) {
    const overview = await read<{ campaigns: OverviewEntry[]; canStart: boolean;
      csrfToken: string; }>('/api/overview');
    if (overview) Object.assign(state, { overview: overview.campaigns,
      canStart: overview.canStart, csrfToken: overview.csrfToken });
    render();
  }
  if (current.plans) {
    const plans = await read<DashboardPlan[]>('/api/plans');
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
      if (sheet) state.sheets.set(campaign.key, sheet);
      render();
    }
    return;
  }
  const sheet = await read<CampaignSheet>(`/api/campaigns/${encodeURIComponent(current.key)}`);
  if (sheet) state.sheets.set(current.key, sheet);
  render();
  if (sheet?.mode === 'dependency' && current.view !== 'grid'
    && !state.progression.has(current.key)) {
    state.progression.set(current.key, await read<CampaignProgression>(
      `/api/campaigns/${encodeURIComponent(current.key)}/progression`));
    render();
  }
  if (!current.attempt) return;
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
  void load();
}

function stepTo(offset: number): void {
  const current = route();
  const progression = state.progression.get(current.key) ?? null;
  if (!progression) return;
  const total = replayTimeline(progression).length;
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

document.addEventListener('click', event => {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey
    || event.shiftKey || event.altKey) return;
  const link = (event.target as Element | null)?.closest('a');
  const href = link?.getAttribute('href') ?? '';
  if (!href || href.startsWith('/api/') || !/^[/?]/.test(href)) return;
  event.preventDefault();
  go(href.startsWith('?') ? `${location.pathname}${href}` : href);
});

// Start and resume are the same request twice: the browser token, the operator
// secret the operator just typed, and the plan the server re-reads itself.
async function post(form: HTMLFormElement): Promise<void> {
  const current = route();
  const data = new FormData(form);
  const resume = form.dataset.run === 'resume';
  const output = resume ? current.key : String(data.get('output') ?? '');
  const response = await fetch(resume
    ? `/api/campaigns/${encodeURIComponent(current.key)}/resume` : '/api/campaigns', {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-stack-bench-token': state.csrfToken,
      'x-stack-bench-control-secret': String(data.get('secret') ?? '') },
    body: JSON.stringify(resume ? {} : { planId: String(data.get('plan') ?? ''), outputName: output }),
  });
  if (response.ok) {
    state.form = { ...state.form, secret: '', error: '' };
    if (resume) return void load();
    return go(`/c/${encodeURIComponent(output)}`);
  }
  const failure = await response.json().catch(() => ({})) as { error?: string };
  state.form = afterRun(state.form, response.status, failure.error ?? '');
  render();
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

window.addEventListener('popstate', () => void load());
subscribe();
void load();
