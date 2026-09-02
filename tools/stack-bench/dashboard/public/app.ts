/// <reference lib="dom" />
/// <reference lib="dom.iterable" />

// The client: real paths, one event stream, and keyed reconciliation so a
// refresh does not move what the pointer is on. Every view is a pure function
// of data; the only DOM work in the dashboard happens here.

import type { AttemptChecks, AttemptPackage, CampaignProgression, CampaignSheet, OverviewEntry }
  from '../dashboard-views.js';
import { campaignPage, replayTimeline } from './views/campaign.js';
import type { QuestlineView } from './views/campaign.js';
import { attemptPage } from './views/attempt.js';
import type { AttemptTab } from './views/attempt.js';
import { campaignsPage } from './views/campaigns.js';
import type { CampaignFilter } from './views/campaigns.js';
import { esc } from './format.js';

const FALLBACK_MS = 15_000;
const TABS: readonly AttemptTab[] = ['checks', 'screenshots', 'files', 'log'];
const VIEWS: readonly QuestlineView[] = ['grid', 'graph', 'replay'];
const FILTERS: readonly CampaignFilter[] = ['all', 'attention', 'completed', 'ready'];

interface Route {
  key: string;
  attempt: string;
  filter: CampaignFilter;
  view: QuestlineView;
  step: number;
  tab: AttemptTab;
}

const state = {
  overview: [] as OverviewEntry[],
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

function topbar(current: Route): string {
  const artifact = (path: string): string =>
    `/api/campaigns/${encodeURIComponent(current.key)}/artifacts/`
    + btoa(path).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const files = current.key && !current.attempt
    ? `<details class="files"><summary class="btn">Files</summary><div>`
      + `<a href="${artifact('plan.json')}">plan</a>`
      + `<a href="${artifact('state.json')}">state</a>`
      + `<a href="${artifact('report/report.html')}">report</a></div></details>` : '';
  return '<div class="topbar"><a class="brand" href="/">'
    + '<img src="/spacetimedb-mark.svg" alt="" width="26" height="24">'
    + `<b>STACK BENCH</b></a>${files}</div>`;
}

function page(current: Route): string {
  const sheet = state.sheets.get(current.key) ?? null;
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
  next.innerHTML = `${topbar(current)}<main>${page(current)}</main>`;
  patch(root, next);
}

async function load(): Promise<void> {
  const current = route();
  if (!current.key) {
    const overview = await read<{ campaigns: OverviewEntry[] }>('/api/overview');
    if (overview) state.overview = overview.campaigns;
    render();
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
