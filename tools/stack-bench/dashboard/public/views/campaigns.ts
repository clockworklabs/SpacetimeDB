// The campaign history, with any reference run in progress above it.

import type { OverviewCampaign, OverviewEntry, OverviewPage } from '../../dashboard-views.js';
import { DASH, esc, pct, shape, since, stackLabel, statusWord } from '../format.js';
import type { ReferenceRun } from '../../dashboard-reference-runs.js';

import type { CampaignFilter } from '../../dashboard-views.js';
export type { CampaignFilter } from '../../dashboard-views.js';

const FILTERS: Array<{ id: CampaignFilter; label: string }> = [{ id: 'all', label: 'All' },
  { id: 'attention', label: 'Needs attention' }, { id: 'completed', label: 'Completed' },
  { id: 'ready', label: 'Ready' }];

function readable(campaign: OverviewEntry): campaign is OverviewCampaign {
  return 'scores' in campaign;
}

function matches(campaign: OverviewEntry, filter: CampaignFilter): boolean {
  if (filter === 'all') return true;
  if (filter === 'attention') {
    return campaign.status === 'attention-required' || campaign.status === 'unreadable';
  }
  if (filter === 'completed') return campaign.status === 'completed';
  return campaign.status === 'prepared';
}

function stackCell(campaign: OverviewEntry, stack: string): string {
  const score = readable(campaign) ? campaign.scores[stack] ?? null : null;
  if (score === null) return `<td class="stack na">${DASH}</td>`;
  return `<td class="stack">`
    + `${pct(score)}</td>`;
}

function tone(status: string): string {
  if (status === 'running') return 'run';
  if (status === 'completed') return 'done';
  if (status === 'attention-required' || status === 'unreadable') return 'warn';
  return 'idle';
}

function row(campaign: OverviewEntry, stacks: readonly string[]): string {
  const summary = readable(campaign) ? campaign : null;
  return `<tr data-key="${esc(campaign.key)}"><td class="name">`
    + `<a href="/c/${encodeURIComponent(campaign.key)}">${esc(campaign.title)}</a></td>`
    + `<td class="shape">${summary
      ? esc(shape(summary.mode, summary.levels, summary.repetitions)) : DASH}</td>`
    + `<td><span class="state ${tone(campaign.status)}">${esc(statusWord(campaign.status))}</span></td>`
    + stacks.map(stack => stackCell(campaign, stack)).join('')
    + `<td class="when">${summary ? esc(since(summary.updatedAt)) : DASH}</td></tr>`;
}

export function campaignsPage({ campaigns, filter, loading = false, references, pagination }: {
  campaigns: readonly OverviewEntry[];
  filter: CampaignFilter;
  loading?: boolean;
  references?: { runs: ReferenceRun[]; error: string | null };
  pagination?: Pick<OverviewPage, 'page' | 'pages' | 'total' | 'pageSize' | 'counts'>;
}): string {
  const stacks = [...new Set(campaigns.flatMap(campaign => readable(campaign) ? Object.keys(campaign.scores) : []))];
  const shown = campaigns.filter(campaign => matches(campaign, filter));
  const chips = FILTERS.map(entry =>
    `<a class="chip${entry.id === filter ? ' on' : ''}"${entry.id === filter ? ' aria-current="page"' : ''} href="/?filter=${entry.id}">`
    + `${entry.label}${loading ? '' : ` ${pagination?.counts[entry.id] ?? campaigns.filter(campaign => matches(campaign, entry.id)).length}`}</a>`).join('');
  const body = loading ? `<tr><td colspan="${4 + stacks.length}"><div class="loading" role="status">Loading campaigns…</div></td></tr>`
    : shown.length ? shown.map(campaign => row(campaign, stacks)).join('')
    : `<tr><td colspan="${4 + stacks.length}">No campaigns match this filter.</td></tr>`;
  const pager = pagination && !loading ? `<nav class="toolbar" aria-label="Campaign pages">`
    + (pagination.page > 1 ? `<a class="btn" href="/?filter=${filter}&page=${pagination.page - 1}">Previous</a>` : '')
    + `<span>Page ${pagination.page} of ${pagination.pages} · ${pagination.total} campaigns</span>`
    + (pagination.page < pagination.pages ? `<a class="btn" href="/?filter=${filter}&page=${pagination.page + 1}">Next</a>` : '')
    + '</nav>' : '';
  const validations = references?.runs.filter(run => run.status === 'running')
    .map(run => `<details class="reference-run" data-key="reference:${esc(run.id)}">`
    + `<summary><span class="reference-identity"><b>${esc(run.title)}</b>`
    + `<span class="state run">${esc(run.status)}</span></span><span class="reference-score">${run.points
      ? `<strong>${run.points.passed}${run.points.planned === null ? '' : `/${run.points.planned}`}</strong> points passed`
      : 'Awaiting results'}</span>`
    + `<span class="reference-toggle">Log <span aria-hidden="true">›</span></span></summary>`
    + `<pre class="log">${esc(run.log || 'No log recorded.')}</pre></details>`).join('') ?? '';
  return `<div class="page"><div class="title"><h2>Campaigns</h2></div>`
    + validations
    + (references?.error ? `<p class="err">${esc(references.error)}</p>` : '')
    + `<div class="tablewrap"><div class="toolbar">${chips}</div><div class="wrap">`
    + '<table class="runs"><thead><tr><th>Campaign</th><th>Scope</th><th>Status</th>'
    + stacks.map(stack => `<th class="stack">${esc(stackLabel(stack))}</th>`).join('')
    + `<th class="when">Updated</th></tr></thead><tbody>${body}</tbody></table></div>${pager}</div></div>`;
}
