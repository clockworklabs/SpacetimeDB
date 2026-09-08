// Results compare stacks. Runs expose individual evidence. Feature views use
// the same selected attempt per stack, separate from aggregate results.

import type { CampaignProgression, CampaignSheet, ProgressionStep, SheetAttempt, SheetStack }
  from '../../dashboard-views.js';
import { DASH, duration, executionClock, esc, metricLabel, spend, num, pct, phrase, ratio, stackLabel, statusWord } from '../format.js';
import { progressChart } from '../progress-chart.js';
import { graph } from '../graph.js';

export type QuestlineView = 'grid' | 'graph' | 'replay';

export interface CampaignPageInput {
  sheet: CampaignSheet;
  progression: CampaignProgression | null;
  view: QuestlineView;
  chart?: 'completion' | 'cost';
  step: number;
}

export interface ReplayEvent {
  stack: string;
  ordinal: number;
  step: ProgressionStep;
}

const DOT: Record<string, string> = { passed: 'p', active: 'a', working: 'a', failed: 'f',
  blocked: 'b', locked: 'o' };

function short(value: string | null): string {
  return value ? value.slice(0, 12) : DASH;
}

function latest(stack: SheetStack): SheetAttempt | null {
  return stack.attempts.find(attempt => attempt.id === stack.selectedAttemptId) ?? null;
}

function facts(sheet: CampaignSheet): string {
  const fact = sheet.facts;
  const dependency = sheet.mode === 'dependency';
  const depth = sheet.levels.length ? Math.max(...sheet.levels) : 0;
  const cells: Array<[string, string, string]> = [['Mode', fact.mode, '']];
  const limits = fact.repairLimits;
  const repairBudget = [
    limits.perFeature === undefined ? '' : `${limits.perFeature} per feature`,
    limits.perDepth === undefined ? '' : `${limits.perDepth.count} per depth${limits.perDepth.carry ? " (carry forward)" : ""}`,
    limits.total === undefined ? '' : `${limits.total} total`,
  ].filter(Boolean).join(' · ');
  cells.push(dependency ? ['Depth', String(depth), '']
    : ['Levels', sheet.levels.map(level => `L${level}`).join('–'), '']);
  if (dependency) {
    cells.push(['Work', fact.workSelection ?? DASH, ''],
      ['Repair', fact.repairSelection ?? DASH, ''],
      ['Repair budget', repairBudget || 'No count limit', 'Limits apply to each attempt. When limits overlap, the tightest remaining limit applies.']);
  } else {
    cells.push(['Repair budget', repairBudget || 'No count limit', 'Limits apply to each attempt.']);
  }
  cells.push(['Repetitions', String(sheet.repetitions), ''],
    ['Agent', fact.agent ?? DASH, ''], ['Model', fact.model ?? DASH, ''],
    ['Guidance', fact.guidance ?? DASH, ''],
    ['Recipe', [...new Set(fact.recipes.map(recipe =>
      [recipe.id, short(recipe.contentSha256)].filter(Boolean).join(' ')))].join(' · ') || DASH, ''],
    ['Time limit', `${fact.timeLimitMinutes} min`, ''],
    ['Spend limit', fact.spendLimitUsd === null ? DASH
      : `$${fact.spendLimitUsd} per attempt`, ''],
    ['Controller', short(fact.controllerImage), ''], ['Plan', short(fact.planSha256), '']);
  if (sheet.mixedScope) cells.push(['Scope', 'mixed', 'attempts do not share one test plan']);
  const continued = sheet.stacks.filter(stack => stack.continued).length;
  if (continued) cells.push(['Continued', String(continued), '']);

  return `<div class="facts">${cells.map(([label, value, hover]) =>
    `<div><span class="label">${esc(label)}</span><b title="${esc(hover || value)}">`
    + `${esc(value)}</b></div>`).join('')}</div>`;
}

function questlineRows(sheet: CampaignSheet, stacks: readonly SheetStack[]): string {
  const lead = stacks.find(stack => stack.questlines?.length)?.questlines ?? [];
  const rows = lead.map(questline => {
    const cells = stacks.map(stack => {
      const owned = stack.questlines?.find(entry => entry.id === questline.id) ?? null;
      const dots = (owned?.nodes ?? []).map(node =>
        `<i class="dot ${DOT[node.status] ?? 'o'}" title="${esc(`${node.id}: ${statusWord(node.status)}`)}"></i>`).join('');
      const score = owned?.score ?? null;
      return `<td><div class="q">${dots}<span class="pct${score === 100 ? ' full' : ''}">`
        + `${pct(score)}</span></div></td>`;
    }).join('');
    return `<tr><th scope="row" class="q k">${esc(questline.title)}</th>${cells}</tr>`;
  }).join('');
  if (sheet.mode !== 'dependency') return '';
  return rows || `<tr><td colspan="${stacks.length + 1}" class="summary-note">Feature progress appears after the first recorded grade.</td></tr>`;
}

function levelRows(stacks: readonly SheetStack[]): string {
  const levels = stacks.find(stack => stack.levels?.length)?.levels ?? [];
  return levels.map(level => ['unaided', 'score'].map(kind => {
    const cells = stacks.map(stack => {
      const owned = stack.levels?.find(entry => entry.level === level.level) ?? null;
      const points = kind === 'unaided' ? owned?.unaided ?? null : owned?.score ?? null;
      return `<td><div class="q"><span class="v">${points
        ? ratio(points.score, points.max) : DASH}</span></div></td>`;
    }).join('');
    return `<tr><th scope="row" class="q k">L${level.level} ${kind}</th>${cells}</tr>`;
  }).join('')).join('');
}

export function selectedProgression(progression: CampaignProgression, sheet: CampaignSheet): CampaignProgression {
  return { ...progression, stacks: sheet.stacks.flatMap(stack => progression.stacks.filter(track =>
    stack.stack === track.stack && stack.selectedAttemptId === track.attemptId)) };
}

export function replayTimeline(progression: CampaignProgression): ReplayEvent[] {
  const tracks = progression.stacks;
  const depth = Math.max(0, ...tracks.map(track => track.steps.length));
  const events: ReplayEvent[] = [];
  for (let ordinal = 0; ordinal < depth; ordinal += 1) {
    for (const track of tracks) {
      const step = track.steps[ordinal];
      if (step) events.push({ stack: track.stack, ordinal, step });
    }
  }
  return events;
}

function marker(step: ProgressionStep, failed: boolean): string {
  if (failed) return 'f';
  if (step.action === 'repair') return 'r';
  return step.action === 'grant' ? 'g' : 'b';
}

function replay(progression: CampaignProgression, cursor: number): string {
  const events = replayTimeline(progression);
  cursor = Math.min(Math.max(0, cursor), Math.max(0, events.length - 1));
  const span = Math.max(1, events.length - 1);
  const selected = events[cursor] ?? events.at(-1) ?? null;
  const failedAt = (step: ProgressionStep): boolean => step.targets.some(target =>
    step.statuses[progression.nodes.findIndex(node => node.id === target)] === 'failed');
  const title = (id: string): string =>
    progression.nodes.find(node => node.id === id)?.title ?? id;
  const head = selected ? [['Step', ratio(cursor + 1, events.length)],
    ['Stack', esc(stackLabel(selected.stack))], ['Action', esc(selected.step.action)],
    ['Feature', selected.step.targets.length === 1
      ? esc(title(selected.step.targets[0] ?? '')) : `${selected.step.targets.length} features`],
    ['Score', pct(selected.step.score)], ['Repairs', num(selected.step.repairs)]]
    .map(([label, value]) => `<div class="ev"><span class="label">${label}</span>`
      + `<span class="v">${value}</span></div>`).join('') : '';
  // Drawn as one SVG per stack: the dashboard's policy allows no inline style,
  // and a marker's position is geometry, not decoration.
  const at = (index: number): number => 20 + 960 * index / span;
  const rows = progression.stacks.map(track => {
      const marks = events.map((event, index) => event.stack !== track.stack ? '' :
        `<rect class="st ${marker(event.step, failedAt(event.step))}`
        + `${index > cursor ? ' dim' : ''}${index === cursor ? ' on' : ''}" `
        + `x="${at(index).toFixed(1)}" y="9" width="10" height="10" rx="2"/>`).join('');
      return `<tr><th scope="row" class="k">${esc(stackLabel(track.stack))}</th><td class="replay">`
        + '<svg class="strip" viewBox="0 0 1000 28">'
        + `<line class="cur" x1="${at(cursor).toFixed(1)}" y1="2" `
        + `x2="${at(cursor).toFixed(1)}" y2="26"/>${marks}</svg></td></tr>`;
    }).join('');
  const snapshot = progression.stacks.map(track => {
      const step = events.filter((event, index) =>
        event.stack === track.stack && index <= cursor).at(-1)?.step ?? null;
      return { stack: track.stack,
        statuses: step?.statuses ?? progression.nodes.map(() => 'locked') };
    });
  return `<div class="evhead">${head}</div>`
    + graph(progression, snapshot)
    + `<div class="sheet-scroll"><table class="sheet"><tbody>${rows}</tbody></table></div>`;
}

function board({ sheet, progression, view, step }: CampaignPageInput,
  stacks: readonly SheetStack[]): string {
  const chips = (['grid', 'graph', 'replay'] as const).map(entry =>
    `<a class="chip sm${entry === view ? ' on' : ''}"${entry === view ? ' aria-current="page"' : ''} href="?questlines=${entry}">`
    + `${entry[0]!.toUpperCase()}${entry.slice(1)}</a>`).join('');
  const heading = stacks.map(stack => `<th scope="col" class="h" title="${latest(stack) ? `Rep ${latest(stack)!.repetition}` : 'Not started'}">${esc(stackLabel(stack.stack))}</th>`).join('');
  const grid = (rows: string): string => `<div class="sheet-scroll" role="region" aria-label="Feature progress grid" tabindex="0"><table class="sheet"><thead><tr><th scope="col" class="h">Feature</th>${heading}</tr></thead><tbody>${rows}</tbody></table></div>`;
  let content: string;
  if (sheet.mode !== 'dependency') content = grid(levelRows(stacks));
  else if (view === 'grid' || !progression) content = grid(questlineRows(sheet, stacks));
  else if (view === 'graph') {
    const selected = selectedProgression(progression, sheet);
    const snapshot = selected.stacks.map(track => ({ stack: track.stack,
        statuses: track.steps.at(-1)?.statuses ?? selected.nodes.map(() => 'locked') }));
    content = graph(selected, snapshot);
  } else content = replay(selectedProgression(progression, sheet), step);
  return '<section class="feature-progress" aria-labelledby="feature-progress-title">'
    + '<div class="section-heading"><h3 id="feature-progress-title" title="Latest started repetition per stack">Feature progress</h3>'
    + (sheet.mode === 'dependency' ? `<details class="explore"><summary>Explore · ${esc(view)}</summary><nav aria-label="Feature progress view">${chips}</nav></details>` : '')
    + '</div>'
    + content + '</section>';
}

export function campaignPage(input: CampaignPageInput): string {
  const sheet = input.sheet;
  const stacks = sheet.stacks;
  const cell = (render: (stack: SheetStack) => string): string =>
    stacks.map(stack => `<td>${render(stack)}</td>`).join('');
  const help: Record<string, string> = {
    Completion: 'Median checks passed divided by all selected checks, including checks not yet reached.',
    'Weighted score': 'Score weighted by check points. Final comparison values appear when usable attempts finish.',
    Unaided: 'Score before repair, based on the recorded first-try evidence.',
    Regressions: 'Median regression count across recorded repetitions. A regression is a previously passing check that failed after a later change.',
    'Valid runs': 'Completed attempts with usable comparison evidence. A completed process alone does not guarantee a usable result.',
    Excluded: 'Attempts omitted from comparison because their evidence is invalid or incomplete. Their costs appear only in total spend and individual run details.',
    Time: 'Median duration of usable completed attempts. Live attempt status appears below.',
    'Cost per valid run': 'Mean cost of completed runs with valid comparison results. Excludes invalid and unfinished runs. Valid does not mean every check passed. All included runs must have exact cost evidence and the same comparison scope.',
    'Total spend': 'Includes excluded attempts. During a run, this includes only saved cost receipts. Includes saved repair checkpoints in the active depth. Work since the last checkpoint is not yet counted. Unknown is not zero. Costs use the pinned price snapshot.',
  };
  const row = (label: string, render: (stack: SheetStack) => string): string =>
    `<tr><th scope="row" class="k">${metricLabel(label, help[label])}</th>${cell(render)}</tr>`;
  const value = (text: string): string => `<div class="v">${text}</div>`;
  const heads = stacks.map(stack => {
    const attempt = latest(stack);
    const label = esc(stackLabel(stack.stack));
    return `<th scope="col" class="h">${attempt
      ? `<a href="/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}">`
        + `${label}</a>` : label}</th>`;
  }).join('');
  const repetitions = row('Valid runs', stack => value(ratio(stack.n, stack.attempts.length)))
      + row('Excluded', stack =>
        value(num(stack.attempts.filter(attempt => attempt.excluded).length)));
  return `<div class="page"><div class="crumbs"><a href="/">Campaigns</a> / `
    + `<b>${esc(sheet.key)}</b></div>`
    + `<div class="title"><h2>${esc(sheet.title)}</h2>`
    + `<span class="state${sheet.provisional ? ' warn' : ''}" title="${esc([statusWord(sheet.status), sheet.facts.grading, ...sheet.facts.gradingReasons].join(' · '))}">${sheet.provisional ? 'Provisional' : esc(statusWord(sheet.status))}</span></div>${facts(sheet)}`
    + '<h3>Results</h3>'
    + `<div class="sheet-scroll" role="region" aria-label="Stack comparison" tabindex="0"><table class="sheet"><thead><tr><th scope="col" class="h">Metric</th>${heads}</tr></thead><tbody>`
    + row('Completion', stack => `<div class="big">${pct(stack.completionRate === null ? null : 100 * stack.completionRate)}</div>`)
    + row('Cost per valid run', stack => value(stack.costPerValidRun === null ? (stack.n ? 'Unknown' : 'Awaiting valid runs') : `$${stack.costPerValidRun.toFixed(2)}`))
    + row('Weighted score', stack => value(pct(stack.score)))
    + row('Unaided', stack => value(pct(stack.unaided)))
    + row('Regressions', stack => value(num(stack.regressions)))
    + row('Time', stack => value(duration(stack.timeSec)))
    + repetitions
    + row('Total spend', stack => value(spend(stack.spend) + (stack.spendPending ? ' (so far)' : '')))
    + '</tbody></table></div>'
    + (sheet.mode === 'dependency' ? progressChart(sheet, input.progression, input.chart, input.view) : '')
    + '<h3>Runs</h3>'
    + '<div class="tablewrap"><div class="wrap"><table class="runs attempt-list"><thead><tr><th>Run</th><th>Completion</th><th>Spend</th><th>Repairs</th><th>Elapsed</th><th>Status</th></tr></thead><tbody>'
    + stacks.flatMap(stack => stack.attempts.map(attempt => {
      const href = `/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}`;
      return `<tr><td><a href="${href}" title="${esc(attempt.variant)}">${esc(stackLabel(stack.stack))} · Rep ${attempt.repetition}</a></td>`
        + `<td>${attempt.completion ? ratio(attempt.completion.passed, attempt.completion.selected) : DASH}</td>`
        + `<td title="Saved cost receipts through the latest grade checkpoint, including repairs. Work since that checkpoint is not yet counted.">${spend(attempt.spend)}${attempt.spendPending ? ' (so far)' : ''}</td>`
        + `<td>${ratio(attempt.repairs.used, attempt.repairs.budget)}</td>`
        + `<td>${attempt.status === 'running' || attempt.executionCompletedAt ? executionClock(attempt.executionStartedAt, attempt.executionCompletedAt) : DASH}</td>`
        + `<td class="run-status">${attempt.excluded
          ? `<details><summary>Excluded · show reason</summary><p>${esc(attempt.excluded)}</p></details>`
          : esc(phrase(attempt))}</td></tr>`;
    })).join('')
    + '</tbody></table></div></div>' + board(input, stacks) + '</div>';
}
