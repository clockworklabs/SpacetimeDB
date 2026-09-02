// Campaigns: a live block per running campaign, then one table of every
// campaign. Nothing here explains itself — a lane is four cells, a row is
// seven, and the status word lives in the Status column.

import type { CampaignSheet, OverviewCampaign, OverviewEntry, SheetAttempt }
  from '../../dashboard-views.js';
import { climb } from '../climb.js';
import { DASH, esc, pct, phrase, shape, since, stackLabel, statusWord } from '../format.js';
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

// Percentage of the points the grades have offered so far, which is the only
// score a half-finished attempt has.
function running(attempt: SheetAttempt): number | null {
  const last = attempt.climb.at(-1);
  return last && last.max ? 100 * last.score / last.max : attempt.score;
}

function lane(sheet: CampaignSheet, stack: string, attempt: SheetAttempt): string {
  const warn = attempt.stalling;
  return `<div class="lane" data-key="${esc(`${sheet.key}:${attempt.id}`)}">`
    + `<span class="who">${esc(stackLabel(stack))}</span>`
    + `<span class="big${sheet.provisional ? ' prov' : ''}">${pct(running(attempt))}</span>`
    + `<span>${climb(attempt.climb, { warn })}</span>`
    + `<span class="phase${warn ? ' warn' : ''}">${esc(phrase(attempt))}</span></div>`;
}

function live(sheet: CampaignSheet): string {
  const lanes = STACK_ORDER.flatMap(stack => {
    const owner = sheet.stacks.find(entry => entry.stack === stack);
    const attempt = owner?.attempts.findLast(item => item.status === 'running');
    return attempt ? [lane(sheet, stack, attempt)] : [];
  });
  if (!lanes.length) return '';
  return `<div class="live" data-key="${esc(sheet.key)}"><div class="live-head">`
    + `<b>${esc(sheet.title)}</b></div>${lanes.join('')}</div>`;
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
    `<a class="chip${entry.id === filter ? ' on' : ''}" href="/?filter=${entry.id}">`
    + `${entry.label} ${campaigns.filter(campaign => matches(campaign, entry.id)).length}</a>`).join('');
  const body = shown.length ? shown.map(row).join('')
    : `<tr><td colspan="7">no campaigns</td></tr>`;
  return `<div class="page">${sheets.map(live).join('')}`
    + `<div class="tablewrap"><div class="toolbar">${chips}</div><div class="wrap">`
    + '<table class="runs"><thead><tr><th>Campaign</th><th>Shape</th><th>Status</th>'
    + STACK_ORDER.map(stack => `<th class="stack">${esc(stackLabel(stack))}</th>`).join('')
    + `<th class="when">Updated</th></tr></thead><tbody>${body}</tbody></table></div></div></div>`;
}
