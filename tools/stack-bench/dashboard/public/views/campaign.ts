// One campaign is one sheet: stacks across in fixed order, facts down, one
// value per cell. The questline rows are the grid; Graph and Replay replace
// them with a full-width cell drawn by the shared graph renderer.

import type { CampaignProgression, CampaignSheet, ProgressionStep, SheetAttempt, SheetStack }
  from '../../dashboard-views.js';
import { climb } from '../climb.js';
import { DASH, duration, esc, money, num, pct, phrase, ratio, stackLabel } from '../format.js';
import { STACK_ORDER } from '../metrics.js';
import { graph } from '../graph.js';

export type QuestlineView = 'grid' | 'graph' | 'replay';

export interface CampaignPageInput {
  sheet: CampaignSheet;
  progression: CampaignProgression | null;
  view: QuestlineView;
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

function ordered(sheet: CampaignSheet): SheetStack[] {
  return STACK_ORDER.flatMap(stack => sheet.stacks.filter(entry => entry.stack === stack));
}

function latest(stack: SheetStack): SheetAttempt | null {
  return stack.attempts.findLast(attempt => attempt.status === 'running')
    ?? stack.attempts.at(-1) ?? null;
}

function facts(sheet: CampaignSheet): string {
  const fact = sheet.facts;
  const dependency = sheet.mode === 'dependency';
  const depth = sheet.levels.length ? Math.max(...sheet.levels) : 0;
  const cells: Array<[string, string, string]> = [['Mode', fact.mode, '']];
  cells.push(dependency ? ['Depth', String(depth), '']
    : ['Levels', sheet.levels.map(level => `L${level}`).join('–'), '']);
  if (dependency) {
    cells.push(['Work', fact.workSelection ?? DASH, ''],
      ['Repair', fact.repairSelection ?? DASH, ''],
      ['Strikes', fact.strikes === null ? DASH
        : `${fact.strikes} per ${fact.strikePolicy ?? 'feature'}`, '']);
  } else {
    cells.push(['Repairs', `${fact.fixRounds} rounds per level`, '']);
  }
  cells.push(['Repetitions', String(sheet.repetitions), ''],
    ['Agent', fact.agent ?? DASH, ''], ['Model', fact.model ?? DASH, ''],
    ['Guidance', fact.guidance ?? DASH, ''],
    [fact.recipes.length > 1 ? 'Recipes' : 'Recipe', fact.recipes.map(recipe =>
      [recipe.id, recipe.version].filter(Boolean).join(' ')).join(' · ') || DASH, ''],
    ['Time limit', `${fact.timeLimitMinutes} min`, ''],
    ['Spend limit', fact.spendLimitUsd === null ? DASH
      : `$${fact.spendLimitUsd} per attempt`, ''],
    ['Controller', short(fact.controllerImage), ''], ['Plan', short(fact.planSha256), ''],
    ['Grading', fact.grading, fact.gradingReasons.join(' · ')]);
  if (sheet.mixedScope) cells.push(['Scope', 'mixed', 'attempts do not share one test plan']);
  const continued = sheet.stacks.filter(stack => stack.continued).length;
  if (continued) cells.push(['Continued', String(continued), '']);
  return `<div class="facts">${cells.map(([label, value, hover]) =>
    `<div><span class="label">${esc(label)}</span><b title="${esc(hover || value)}"`
    + `${label === 'Grading' && value !== 'qualified' ? ' class="warn"' : ''}>`
    + `${esc(value)}</b></div>`).join('')}</div>`;
}

function questlineRows(sheet: CampaignSheet, stacks: readonly SheetStack[]): string {
  const lead = stacks.find(stack => stack.questlines?.length)?.questlines ?? [];
  const rows = lead.map(questline => {
    const cells = stacks.map(stack => {
      const owned = stack.questlines?.find(entry => entry.id === questline.id) ?? null;
      const dots = (owned?.nodes ?? []).map(node =>
        `<i class="dot ${DOT[node.status] ?? 'o'}"></i>`).join('');
      const score = owned?.score ?? null;
      return `<div class="q">${dots}<span class="pct${score === 100 ? ' full' : ''}">`
        + `${pct(score)}</span></div>`;
    }).join('');
    return `<div class="q k">${esc(questline.title)}</div>${cells}`;
  }).join('');
  const average = stacks.map(stack =>
    `<div class="band sum"><span class="v">${pct(stack.score)}</span></div>`).join('');
  if (sheet.mode !== 'dependency') return '';
  return `${rows}<div class="k band sum">Questline average</div>${average}`;
}

function levelRows(stacks: readonly SheetStack[]): string {
  const levels = stacks.find(stack => stack.levels?.length)?.levels ?? [];
  return levels.map(level => ['unaided', 'score'].map(kind => {
    const cells = stacks.map(stack => {
      const owned = stack.levels?.find(entry => entry.level === level.level) ?? null;
      const points = kind === 'unaided' ? owned?.unaided ?? null : owned?.score ?? null;
      return `<div class="q"><span class="v">${points
        ? ratio(points.score, points.max) : DASH}</span></div>`;
    }).join('');
    return `<div class="q k">L${level.level} ${kind}</div>${cells}`;
  }).join('')).join('');
}

export function replayTimeline(progression: CampaignProgression): ReplayEvent[] {
  const tracks = STACK_ORDER.flatMap(stack =>
    progression.stacks.filter(track => track.stack === stack));
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
    ['Score', pct(selected.step.score)], ['Strike', num(selected.step.strikes)]]
    .map(([label, value]) => `<div class="ev"><span class="label">${label}</span>`
      + `<span class="v">${value}</span></div>`).join('') : '';
  // Drawn as one SVG per stack: the dashboard's policy allows no inline style,
  // and a marker's position is geometry, not decoration.
  const at = (index: number): number => 20 + 960 * index / span;
  const rows = STACK_ORDER.flatMap(stack => progression.stacks
    .filter(track => track.stack === stack)
    .map(track => {
      const marks = events.map((event, index) => event.stack !== track.stack ? '' :
        `<rect class="st ${marker(event.step, failedAt(event.step))}`
        + `${index > cursor ? ' dim' : ''}${index === cursor ? ' on' : ''}" `
        + `x="${at(index).toFixed(1)}" y="9" width="10" height="10" rx="2"/>`).join('');
      return `<div class="k">${esc(stackLabel(track.stack))}</div><div class="replay span3">`
        + '<svg class="strip" viewBox="0 0 1000 28">'
        + `<line class="cur" x1="${at(cursor).toFixed(1)}" y1="2" `
        + `x2="${at(cursor).toFixed(1)}" y2="26"/>${marks}</svg></div>`;
    })).join('');
  const snapshot = STACK_ORDER.flatMap(stack => progression.stacks
    .filter(track => track.stack === stack).map(track => {
      const step = events.filter((event, index) =>
        event.stack === track.stack && index <= cursor).at(-1)?.step ?? null;
      return { stack: track.stack,
        statuses: step?.statuses ?? progression.nodes.map(() => 'locked') };
    }));
  return `<div class="wide evhead">${head}</div>`
    + `<div class="wide">${graph(progression, snapshot)}</div>${rows}`;
}

function board({ sheet, progression, view, step }: CampaignPageInput,
  stacks: readonly SheetStack[]): string {
  const chips = (['grid', 'graph', 'replay'] as const).map(entry =>
    `<a class="chip sm${entry === view ? ' on' : ''}" href="?questlines=${entry}">`
    + `${entry[0]!.toUpperCase()}${entry.slice(1)}</a>`).join('');
  const switcher = `<div class="k views">${chips}</div><div></div><div></div><div></div>`;
  if (sheet.mode !== 'dependency') return levelRows(stacks);
  if (view === 'grid' || !progression) return switcher + questlineRows(sheet, stacks);
  if (view === 'graph') {
    const snapshot = STACK_ORDER.flatMap(stack => progression.stacks
      .filter(track => track.stack === stack)
      .map(track => ({ stack: track.stack,
        statuses: track.steps.at(-1)?.statuses ?? progression.nodes.map(() => 'locked') })));
    return `${switcher}<div class="wide">${graph(progression, snapshot)}</div>`;
  }
  return switcher + replay(progression, step);
}

export function campaignPage(input: CampaignPageInput): string {
  const sheet = input.sheet;
  const stacks = ordered(sheet);
  const cell = (render: (stack: SheetStack) => string): string =>
    stacks.map(stack => render(stack)).join('');
  const row = (label: string, render: (stack: SheetStack) => string): string =>
    `<div class="k">${esc(label)}</div>${cell(render)}`;
  const value = (text: string): string => `<div class="v">${text}</div>`;
  const heads = stacks.map(stack => {
    const attempt = latest(stack);
    const label = esc(stackLabel(stack.stack));
    return `<div class="h">${attempt
      ? `<a href="/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}">`
        + `${label}</a>` : label}</div>`;
  }).join('');
  const repetitions = sheet.repetitions > 1
    ? row('Completed', stack => value(num(stack.n)))
      + row('Excluded', stack =>
        value(num(stack.attempts.filter(attempt => attempt.excluded).length)))
    : '';
  const evidence = row('Evidence', stack => {
    const attempt = latest(stack);
    if (!attempt) return `<div class="links">${DASH}</div>`;
    const base = `/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}`;
    return `<div class="links">${['screenshots', 'files', 'log'].map(tab =>
      `<a href="${base}?tab=${tab}">${tab}</a>`).join('')}</div>`;
  });
  const issues = stacks.some(stack => stack.attempts.some(attempt => attempt.excluded))
    ? row('Excluded because', stack => {
      const excluded = stack.attempts.filter(attempt => attempt.excluded);
      return excluded.length ? `<div class="reasons">${excluded.map(attempt => {
        const href = `/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}`;
        return `<div><a href="${href}">rep ${attempt.repetition}</a>`
          + `<span title="${esc(attempt.excluded)}">${esc(attempt.excluded)}</span></div>`;
      }).join('')}</div>` : value(DASH);
    }) : '';
  return `<div class="page"><div class="crumbs"><a href="/">Campaigns</a> / `
    + `<b>${esc(sheet.key)}</b></div>`
    + `<div class="title"><h2>${esc(sheet.title)}</h2></div>${facts(sheet)}`
    + `<div class="sheet"><div class="h"></div>${heads}`
    + row('Score', stack => `<div class="big${sheet.provisional ? ' prov' : ''}">`
      + `${pct(stack.score)}</div>`)
    + row('Unaided', stack => value(pct(stack.unaided)))
    + row('Repairs', stack => value(ratio(stack.repairs.used, stack.repairs.budget)))
    + row('Regressions', stack => value(num(stack.regressions)))
    + row('Time', stack => value(duration(stack.timeSec)))
    + row('Spend', stack => value(money(stack.spendUsd)))
    + row('Climb', stack => `<div class="chart">${climb(stack.climb, { height: 44 })}</div>`)
    + row('Attempt', stack => {
      const attempt = latest(stack);
      return `<div><span class="phase${attempt?.stalling ? ' warn' : ''}">`
        + `${attempt ? esc(phrase(attempt)) : DASH}</span></div>`;
    })
    + issues + repetitions + board(input, stacks) + evidence + '</div></div>';
}
