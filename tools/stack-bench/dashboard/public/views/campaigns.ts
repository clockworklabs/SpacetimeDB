// Show every active attempt before the campaign history.

import type { CampaignSheet, OverviewCampaign, OverviewEntry, SheetAttempt }
  from '../../dashboard-views.js';
import { DASH, esc, pct, phrase, shape, since, spend, stackLabel, statusWord } from '../format.js';
import { STACK_ORDER } from '../metrics.js';

export type CampaignFilter = 'all' | 'attention' | 'completed' | 'ready';

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

function lane(sheet: CampaignSheet, stack: string, attempt: SheetAttempt): string {
  const warn = attempt.stalling;
  return `<div class="lane" data-key="${esc(`${sheet.key}:${attempt.id}`)}">`
    + `<a class="who" href="/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}"`
    + ` title="${esc(`${attempt.variant} · repetition ${attempt.repetition}`)}">${esc(stackLabel(stack))} · Rep ${attempt.repetition}</a>`
    + `<span class="big${sheet.provisional ? ' prov' : ''}" title="Checks passed / all selected checks">`
    + `${pct(attempt.completion?.rate == null ? null : 100 * attempt.completion.rate)}</span>`
    + `<span class="phase" title="${esc(attempt.variant)}">${spend(attempt.spend) + (attempt.spendPending ? ' (usage pending)' : '')} · ${esc(attempt.variant)}</span>`
    + `<span class="phase${warn ? ' warn' : ''}">${esc(phrase(attempt))}</span></div>`;
}

function live(sheet: CampaignSheet): string {
  const lanes = STACK_ORDER.flatMap(stack => {
    const owner = sheet.stacks.find(entry => entry.stack === stack);
    return owner?.attempts.filter(item => item.status === 'running')
      .map(attempt => lane(sheet, stack, attempt)) ?? [];
  });
  if (!lanes.length) return '';
  return `<div class="live" data-key="${esc(sheet.key)}"><div class="live-head">`
    + `<b><a href="/c/${encodeURIComponent(sheet.key)}">${esc(sheet.title)}</a></b></div>${lanes.join('')}</div>`;
}

function stackCell(campaign: OverviewEntry, stack: string, best: number | null): string {
  const score = readable(campaign) ? campaign.scores[stack] ?? null : null;
  if (score === null) return `<td class="stack na">${DASH}</td>`;
  const value = best !== null && score === best ? `<u>${pct(score)}</u>` : pct(score);
  return `<td class="stack${readable(campaign) && campaign.provisional ? ' prov' : ''}">`
    + `${value}</td>`;
}

function tone(status: string): string {
  if (status === 'running') return 'run';
  if (status === 'completed') return 'done';
  if (status === 'attention-required' || status === 'unreadable') return 'warn';
  return 'idle';
}

function row(campaign: OverviewEntry): string {
  const summary = readable(campaign) ? campaign : null;
  const best = summary && summary.status === 'completed' && !summary.provisional
    ? STACK_ORDER.reduce<number | null>((top, stack) => {
      const score = summary.scores[stack] ?? null;
      return score !== null && (top === null || score > top) ? score : top;
    }, null) : null;
  return `<tr data-key="${esc(campaign.key)}"><td class="name">`
    + `<a href="/c/${encodeURIComponent(campaign.key)}">${esc(campaign.title)}</a></td>`
    + `<td class="shape">${summary
      ? esc(shape(summary.mode, summary.levels, summary.repetitions)) : DASH}</td>`
    + `<td><span class="state ${tone(campaign.status)}">${esc(statusWord(campaign.status))}</span></td>`
    + STACK_ORDER.map(stack => stackCell(campaign, stack, best)).join('')
    + `<td class="when">${summary ? esc(since(summary.updatedAt)) : DASH}</td></tr>`;
}

export function campaignsPage({ campaigns, sheets, filter }: {
  campaigns: readonly OverviewEntry[];
  sheets: readonly CampaignSheet[];
  filter: CampaignFilter;
}): string {
  const shown = campaigns.filter(campaign => matches(campaign, filter));
  const chips = FILTERS.map(entry =>
    `<a class="chip${entry.id === filter ? ' on' : ''}"${entry.id === filter ? ' aria-current="page"' : ''} href="/?filter=${entry.id}">`
    + `${entry.label} ${campaigns.filter(campaign => matches(campaign, entry.id)).length}</a>`).join('');
  const body = shown.length ? shown.map(row).join('')
    : `<tr><td colspan="7">No campaigns match this filter.</td></tr>`;
  return `<div class="page"><div class="title"><h2>Campaigns</h2></div>${sheets.map(live).join('')}`
    + '<p class="summary-note">Live rows show completion and spend for each running attempt. The table shows median weighted scores from usable completed results. Provisional scores still need qualification. A dash means no usable score yet.</p>'
    + `<div class="tablewrap"><div class="toolbar">${chips}</div><div class="wrap">`
    + '<table class="runs"><thead><tr><th>Campaign</th><th>Scope</th><th>Status</th>'
    + STACK_ORDER.map(stack => `<th class="stack">${esc(stackLabel(stack))}</th>`).join('')
    + `<th class="when">Updated</th></tr></thead><tbody>${body}</tbody></table></div></div></div>`;
}
