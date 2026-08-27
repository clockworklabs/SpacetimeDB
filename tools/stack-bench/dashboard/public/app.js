// Stacks read in a fixed order everywhere, so the same comparison sits in the
// same columns on every refresh whatever order the scheduler started them in.
const STACK_ORDER = ['spacetime', 'postgres', 'mongodb'];
// The same causes campaign-compiler.mjs excludes from a campaign's evidence. A
// result carrying one of these is never counted, never averaged, and never
// silently treated as a low score.
const EXCLUDED_OUTCOMES = new Set(['harness_failure', 'inconclusive', 'ungraded', 'contaminated']);
// Below this many usable repetitions there is no spread worth reporting, so the
// summary says so rather than implying a settled difference.
const MIN_REPETITIONS = 3;
// A finished campaign nobody needs to act on leaves the default view after
// this long. Running and completed campaigns never archive: one is live, the
// other is the record.
const ARCHIVE_AFTER_MS = 72 * 3600 * 1000;

const state = { questlineView: 'questlines', overview: null, csrfToken: null, controlSecret: null,
  selectedPlan: null, showArchived: false,
  openCampaign: null, expanded: new Set(), collapsed: new Set(),
  lastRefreshAt: null, prevScores: new Map() };

const $ = selector => document.querySelector(selector);
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, character => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[character]);
const title = value => ({ postgres: 'PostgreSQL', mongodb: 'MongoDB', spacetime: 'SpacetimeDB',
  sequential: 'Sequential', dependency: 'Dependency' }[value] ?? value);
const statusLabel = value => ({ running: 'Running', completed: 'Completed', prepared: 'Ready',
  pending: 'Queued', 'attention-required': 'Needs attention', interrupted: 'Interrupted',
  invalid: 'Excluded', unreadable: 'Cannot read' }[value] ?? value);
const relativeTime = value => {
  if (!value) return '—';
  const seconds = Math.max(0, (Date.now() - Date.parse(value)) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return new Date(value).toLocaleDateString();
};
const elapsedTime = since => {
  if (!since) return '—';
  const minutes = Math.max(0, Math.floor((Date.now() - Date.parse(since)) / 60000));
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
};
const durationFromSeconds = seconds => {
  if (seconds == null) return '—';
  const minutes = Math.round(seconds / 60);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
};
const plural = (count, noun) => `${count} ${noun}${count === 1 ? '' : 's'}`;
const money = value => value == null ? '—' : `$${value.toFixed(2)}`;
const percent = value => value == null ? '—' : `${Math.round(value * 100)}%`;
const median = values => {
  if (!values.length) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

const skeletonRows = count => Array.from({ length: count }, () => `<tr class="loading">
  <td class="campaign-cell"><span class="skeleton mid"></span></td>
  <td><span class="skeleton short"></span></td>
  <td class="num"><span class="skeleton num"></span></td>
</tr>`).join('');

// The load bar reflects requests actually in flight, so it appears for the
// first paint and for a detail fetch, and never runs on a timer pretending to
// know how long something takes.
let inflight = 0;
function trackLoading(delta) {
  inflight = Math.max(0, inflight + delta);
  $('#loadbar').classList.toggle('active', inflight > 0);
}

async function request(path, options) {
  trackLoading(1);
  try {
    const response = await fetch(path, options);
    const value = await response.json();
    if (!response.ok) {
      const error = new Error(value.error ?? `Request failed (${response.status})`);
      error.status = response.status;
      throw error;
    }
    return value;
  } finally { trackLoading(-1); }
}

async function controlRequest(path, options) {
  state.controlSecret ??= window.prompt('Enter the Stack Bench control secret');
  if (!state.controlSecret) throw new Error('A control secret is required.');
  try {
    return await request(path, { ...options, headers: { ...options.headers,
      'x-stack-bench-token': state.csrfToken,
      'x-stack-bench-control-secret': state.controlSecret } });
  } catch (error) {
    if (error.status === 403) state.controlSecret = null;
    throw error;
  }
}

// ---------------------------------------------------------------- measurement

// Spend is read from the level records because those are written as the run
// proceeds; run.totals only appears once an attempt finishes, so a live attempt
// would otherwise report nothing.
function attemptSpend(attempt) {
  const run = attempt.result;
  if (!run || run.unreadable) return null;
  const levelled = (run.levels ?? []).reduce((total, level) =>
    level.costUsd == null ? total : (total ?? 0) + level.costUsd, null);
  const spend = run.costUsd ?? levelled;
  // A run that recorded no spend reports nothing, not zero. Treating an absent
  // measurement as $0.00 would make it the cheapest stack in the table.
  return spend != null && spend > 0 ? spend : null;
}

// Two numbers per attempt, because they answer different questions: what the
// model got right unaided, and what the whole correction cost. A first grade
// that aborted before grading contributes nothing to the first-build figure —
// an app the grader never scored did not score zero.
function attemptMetrics(attempt) {
  const run = attempt.result;
  if (!run || run.unreadable) return null;
  const levels = (run.levels ?? []).filter(level => level.finalScore);
  if (!levels.length) return null;
  const sum = (list, pick) => list.reduce((total, item) => total + pick(item), 0);
  const scored = levels.filter(level => level.firstScore && !level.firstAbort);
  const abortedFirst = levels.filter(level => level.firstAbort).length;
  const firstMax = sum(scored, level => level.firstScore.max);
  const finalMax = sum(levels, level => level.finalScore.max);
  return {
    first: firstMax ? sum(scored, level => level.firstScore.score) / firstMax : null,
    final: sum(levels, level => level.finalScore.score) / finalMax,
    rounds: sum(levels, level => level.roundsUsed ?? 0),
    spend: attemptSpend(attempt),
    levelsGraded: levels.length,
    abortedFirst,
    // Raw sums over the same set of levels, so a first and a final score shown
    // side by side are always out of the same total.
    raw: { first: firstMax ? { score: sum(scored, l => l.firstScore.score), max: firstMax } : null,
      final: { score: sum(levels, l => l.finalScore.score), max: finalMax } },
  };
}

function attemptExcluded(attempt) {
  const outcome = attempt.execution?.outcome ?? attempt.result?.outcome;
  if (attempt.result?.unreadable) return 'result could not be read';
  if (attempt.status === 'invalid') return attempt.execution?.reason ?? outcome ?? 'excluded';
  // 'ungraded' on an attempt still running means "not yet", not "thrown out".
  if (outcome && EXCLUDED_OUTCOMES.has(outcome) && attempt.status === 'completed') return outcome;
  return null;
}

// Aggregation never crosses a campaign boundary. One campaign is one frozen
// plan, which is the only guarantee the harness gives that two results share a
// recipe, grader, image, model and prompt treatment.
function compareCampaign(campaign) {
  const byStack = new Map();
  for (const attempt of campaign.attempts ?? []) {
    const entry = byStack.get(attempt.stack)
      ?? { stack: attempt.stack, runs: [], excluded: [], pending: 0, spendSoFar: null, abortedFirst: 0 };
    byStack.set(attempt.stack, entry);
    // Every attempt's spend counts toward burn, including ones excluded from
    // the result: a contaminated run still cost money.
    const incurred = attemptSpend(attempt);
    if (incurred != null) entry.spendSoFar = (entry.spendSoFar ?? 0) + incurred;
    const reason = attemptExcluded(attempt);
    if (reason) { entry.excluded.push({ attempt, reason }); continue; }
    const metrics = attempt.status === 'completed' ? attemptMetrics(attempt) : null;
    if (metrics) {
      entry.runs.push({ attempt, metrics });
      entry.abortedFirst += metrics.abortedFirst;
    } else entry.pending += 1;
  }
  const rows = [...byStack.values()]
    .sort((left, right) => STACK_ORDER.indexOf(left.stack) - STACK_ORDER.indexOf(right.stack))
    .map(entry => {
      const pick = key => entry.runs.map(run => run.metrics[key]).filter(value => value != null);
      const range = values => values.length ? { min: Math.min(...values), max: Math.max(...values) } : null;
      const spend = pick('spend');
      const first = pick('first');
      const scopes = [...new Set(entry.runs.map(run => run.metrics.levelsGraded))].sort();
      return { ...entry, n: entry.runs.length, scopes,
        first: median(first), firstRange: range(first),
        final: median(pick('final')),
        rounds: median(pick('rounds')),
        spend: median(spend), spendRange: range(spend) };
    });
  const usable = rows.filter(row => row.n > 0);
  const scopes = new Set(usable.flatMap(row => row.scopes));
  const priced = usable.filter(row => row.spend != null);
  return { rows, usable, priced,
    burn: new Map([...byStack.values()].map(entry => [entry.stack, entry.spendSoFar])),
    // Costs are only comparable when every stack graded the same amount of work.
    mixedScope: scopes.size > 1,
    comparable: priced.length > 1 && scopes.size === 1 };
}

// ------------------------------------------------------------- live rendering

// The attempt's climb, one point per completed grade, normalized by each
// grade's own maximum so L1 and L2 points share a scale.
function sparkline(series, color) {
  if (!series || series.length < 2) return '<span class="spark-empty"></span>';
  const points = series.map((total, index) => {
    const x = (index / (series.length - 1)) * 116 + 2;
    const y = 19 - (total.max ? (total.score / total.max) * 16 : 0);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');
  return `<svg viewBox="0 0 120 22" class="spark" aria-hidden="true">
    <polyline points="${points}" fill="none" stroke="${color}" stroke-width="1.5"/></svg>`;
}

// A trailing run of identical grades is the repair loop treading water.
function stallRounds(series) {
  if (!series || series.length < 4) return 0;
  const last = series.at(-1);
  let flat = 0;
  for (let index = series.length - 2; index >= 0; index--) {
    if (series[index].score === last.score && series[index].max === last.max) flat += 1;
    else break;
  }
  return flat >= 3 ? flat : 0;
}

function outputSilentMinutes(attempt) {
  if (attempt.status !== 'running' || !attempt.logUpdatedAt) return 0;
  return Math.floor((Date.now() - Date.parse(attempt.logUpdatedAt)) / 60000);
}

function laneColor(attempt, stalled) {
  if (stalled || attempt.status === 'interrupted' || attemptExcluded(attempt)) return 'var(--yellow)';
  if (attempt.status === 'completed') return 'var(--blue)';
  if (attempt.status === 'running') return 'var(--green)';
  return 'var(--muted)';
}

function nowLane(attempt) {
  const score = attempt.progress?.latestScore;
  const fraction = score?.max ? score.score / score.max : 0;
  const stalled = attempt.status === 'running' ? stallRounds(attempt.progress?.series) : 0;
  const silent = outputSilentMinutes(attempt);
  const color = laneColor(attempt, stalled || silent >= 10);
  const spend = attemptSpend(attempt);
  const key = `${attempt.id}`;
  const previous = state.prevScores.get(key);
  const changed = score && previous != null && previous !== score.score;
  if (score) state.prevScores.set(key, score.score);
  const notes = [
    stalled ? `flat ${stalled} rounds` : null,
    silent >= 10 ? `no output ${silent}m` : null,
  ].filter(Boolean).join(' · ');
  const detail = attempt.status === 'completed'
    ? `${statusLabel(attempt.status)} · ${attempt.result?.outcome === 'passed' ? 'passed' : 'ended'}`
    : attempt.progress?.phase ?? statusLabel(attempt.status);
  return `<div class="lane">
    <div class="lane-name">${escapeHtml(title(attempt.stack))} <i>r${escapeHtml(attempt.repetition ?? 1)}</i></div>
    <div class="lane-main">
      <div class="lane-bar"><div style="width:${(fraction * 100).toFixed(1)}%;background:${color}"></div></div>
      <div class="lane-detail${notes ? ' warn' : ''}">
        ${escapeHtml(detail)}
        ${score ? ` · <b class="${changed ? 'flash' : ''}">${score.score}/${score.max}</b>` : ''}
        ${notes ? ` · ${escapeHtml(notes)}` : ''}
        ${attempt.status === 'running' ? ` · ${escapeHtml(elapsedTime(attempt.execution?.startedAt))}` : ''}
        ${spend != null ? ` · ${money(spend)}` : ''}
      </div>
    </div>
    ${sparkline(attempt.progress?.series, color)}
  </div>`;
}

function nowEvents(campaign) {
  const events = [];
  for (const attempt of campaign.attempts ?? []) {
    const execution = attempt.execution;
    if (!execution) continue;
    if (execution.completedAt) {
      const score = attempt.progress?.latestScore;
      events.push({ at: execution.completedAt,
        text: `${title(attempt.stack)} r${attempt.repetition ?? 1} finished${score ? ` — ${score.score}/${score.max}` : ''}` });
    } else if (execution.startedAt) {
      events.push({ at: execution.startedAt,
        text: `${title(attempt.stack)} r${attempt.repetition ?? 1} started` });
    }
  }
  const recent = events.sort((left, right) => right.at.localeCompare(left.at)).slice(0, 3);
  if (!recent.length) return '';
  return `<div class="now-events">${recent.map(event =>
    `<div>${escapeHtml(new Date(event.at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }))} · ${escapeHtml(event.text)}</div>`).join('')}</div>`;
}

function nowCard(campaign) {
  const attempts = [...(campaign.attempts ?? [])]
    .sort((left, right) => STACK_ORDER.indexOf(left.stack) - STACK_ORDER.indexOf(right.stack)
      || (left.repetition ?? 1) - (right.repetition ?? 1));
  const done = attempts.filter(attempt => attempt.status === 'completed').length;
  const burn = attempts.reduce((total, attempt) => total + (attemptSpend(attempt) ?? 0), 0);
  const ceiling = campaign.budgets?.maxCostUsdPerAttempt
    ? campaign.budgets.maxCostUsdPerAttempt * attempts.length : null;
  return `<article class="now-card" data-campaign-card="${escapeHtml(campaign.key)}">
    <header>
      <div><button data-campaign="${escapeHtml(campaign.key)}">${escapeHtml(campaign.title)}</button>
        <i class="mono">${escapeHtml(campaign.key)}</i></div>
      <div class="now-figs">${done} of ${attempts.length} done
        · <span class="mono">${money(burn)}${ceiling ? ` / ${money(ceiling)}` : ''}</span>
        · ${escapeHtml(elapsedTime(campaign.createdAt))}</div>
    </header>
    <div class="lanes">${attempts.map(nowLane).join('')}</div>
    ${nowEvents(campaign)}
  </article>`;
}

function renderNow() {
  const running = state.overview.campaigns.filter(campaign => campaign.status === 'running');
  const zone = $('#now-zone');
  zone.hidden = !running.length;
  if (running.length) $('#now-cards').innerHTML = running.map(nowCard).join('');
}

// One dot per stack on a shared 0-100% first-build axis, whiskers for the
// min-max spread, the repaired score as text. The most recent completed
// campaign with graded runs is the verdict until a newer one lands.
function verdictCard(campaign) {
  const summary = compareCampaign(campaign);
  const rows = summary.usable;
  const footnotes = [];
  for (const row of rows) {
    if (row.abortedFirst) footnotes.push(`${title(row.stack)}: ${plural(row.abortedFirst, 'first grade')} aborted before scoring`);
    if (row.excluded.length) footnotes.push(`${title(row.stack)}: ${plural(row.excluded.length, 'attempt')} excluded`);
  }
  if (summary.mixedScope) footnotes.push('stacks graded different amounts of work; costs are not comparable');
  return `<header>
      <button data-campaign="${escapeHtml(campaign.key)}">${escapeHtml(campaign.title)}</button>
      <span>first build, median of runs · whisker = min–max · <time datetime="${escapeHtml(campaign.updatedAt ?? '')}">${escapeHtml(relativeTime(campaign.updatedAt))}</time></span>
    </header>
    <div class="verdict-rows">${rows.map(row => {
    const dot = row.first == null ? null : row.first * 100;
    const min = (row.firstRange?.min ?? row.first ?? 0) * 100;
    const max = (row.firstRange?.max ?? row.first ?? 0) * 100;
    return `<div class="verdict-row">
        <span>${escapeHtml(title(row.stack))}</span>
        <div class="axis">
          <div class="axis-line"></div>
          ${dot == null ? '' : `<div class="whisker" style="left:${min.toFixed(1)}%;width:${Math.max(max - min, 0).toFixed(1)}%"></div>
          <div class="dot" style="left:calc(${dot.toFixed(1)}% - 5px)"></div>`}
        </div>
        <span class="mono">${percent(row.first)} → ${percent(row.final)}${row.spend != null ? ` · ${money(row.spend)}` : ''}</span>
      </div>`;
  }).join('')}</div>
    ${footnotes.length ? `<p class="verdict-notes">${escapeHtml(footnotes.join(' · '))}</p>` : ''}`;
}

function renderVerdict() {
  const latest = state.overview.campaigns
    .filter(campaign => campaign.status === 'completed')
    .sort((left, right) => String(right.updatedAt ?? '').localeCompare(String(left.updatedAt ?? '')))
    .find(campaign => compareCampaign(campaign).usable.length);
  const zone = $('#verdict-zone');
  zone.hidden = !latest;
  if (latest) $('#verdict-card').innerHTML = verdictCard(latest);
}

// ------------------------------------------------------------------ rendering

// One measure, one colour. A per-row hue would double-encode the bar length and
// collide with the status palette, which is the only hue this page spends.
function bar(value, peak, label) {
  if (value == null || !peak) return '<div class="bar"></div>';
  return `<div class="bar"><progress max="${peak}" value="${value}" aria-label="${escapeHtml(label)}"></progress></div>`;
}

function comparisonTable(campaign) {
  const { rows, usable, priced, mixedScope, comparable } = compareCampaign(campaign);
  if (!usable.length) return '<div class="empty">No attempt in this campaign has produced a grade yet.</div>';
  const peakSpend = Math.max(...usable.map(row => row.spend ?? 0), 0);
  const thin = usable.some(row => row.n < MIN_REPETITIONS);
  const notes = [];
  if (priced.length < 2) notes.push(`Only ${plural(priced.length, 'stack')} recorded spend here, so there is no cost comparison to draw.`);
  else if (mixedScope) notes.push('Stacks graded different numbers of levels in this campaign. Their costs measure different amounts of work and are not comparable.');
  if (thin && comparable) notes.push(`Fewer than ${MIN_REPETITIONS} usable runs on at least one stack. Repetitions of the same build have varied by more than the gaps shown here, so read this as a reading, not a result.`);
  for (const row of usable) {
    if (row.abortedFirst) notes.push(`${title(row.stack)}: ${plural(row.abortedFirst, 'first grade')} aborted before scoring — excluded from its first-try figure.`);
  }
  const spread = (range, format) => range && range.min !== range.max
    ? `<small>${format(range.min)}–${format(range.max)}</small>` : '';
  return `<div class="compare-wrap">
    <table class="compare">
      <thead><tr>
        <th scope="col">Stack</th>
        <th scope="col" class="num">First try</th>
        <th scope="col" class="num">After repair</th>
        <th scope="col" class="num">Repairs</th>
        <th scope="col" class="num" colspan="2">Cost to correct<small>lower is better</small></th>
        <th scope="col" class="num">Runs</th>
      </tr></thead>
      <tbody>${rows.map(row => `<tr${row.n ? '' : ' class="unusable"'}>
        <th scope="row">${escapeHtml(title(row.stack))}</th>
        <td class="num">${percent(row.first)}${spread(row.firstRange, percent)}</td>
        <td class="num">${percent(row.final)}</td>
        <td class="num">${row.rounds == null ? '—' : row.rounds}</td>
        <td class="num">${row.spend == null ? '<span class="vacant">no cost recorded</span>' : money(row.spend) + spread(row.spendRange, money)}</td>
        <td class="barcell">${comparable ? bar(row.spend, peakSpend, `${title(row.stack)} median cost to correct`) : ''}</td>
        <td class="num">${row.n}${row.excluded.length ? `<small>${row.excluded.length} excluded</small>` : ''}</td>
      </tr>`).join('')}</tbody>
    </table>
  </div>
  ${notes.map(note => `<p class="caveat">${escapeHtml(note)}</p>`).join('')}
  ${rows.some(row => row.excluded.length) ? `<ul class="exclusions">${rows.flatMap(row => row.excluded.map(item =>
    `<li><strong>${escapeHtml(title(row.stack))} rep ${escapeHtml(item.attempt.repetition ?? '?')}</strong> — ${escapeHtml(item.reason)}</li>`)).join('')}</ul>` : ''}`;
}

function campaignRow(campaign) {
  const running = campaign.status === 'running';
  const live = (campaign.attempts ?? []).filter(attempt =>
    ['running', 'pending'].includes(attempt.status)).length;
  const status = running ? `${live} of ${campaign.attempts.length} running`
    : escapeHtml(statusLabel(campaign.status));
  const stacks = STACK_ORDER.filter(stack =>
    (campaign.attempts ?? []).some(attempt => attempt.stack === stack));
  const shape = stacks.length
    ? `${stacks.map(title).join(' · ')}${campaign.repetitions > 1 ? ` ×${campaign.repetitions}` : ''}`
    : '';
  return `<td class="campaign-cell">
      <span class="chevron${isExpanded(campaign) ? ' open' : ''}" aria-hidden="true"></span>
      <div class="campaign-title">
        <button data-campaign="${escapeHtml(campaign.key)}">${escapeHtml(campaign.title)}</button>
        <i>${escapeHtml(campaign.key)}${shape ? ` · ${escapeHtml(shape)}` : ''}</i>
        ${campaign.error ? `<i class="row-error">${escapeHtml(campaign.error)}</i>` : ''}
      </div>
    </td>
    <td class="state-cell"><span class="status ${escapeHtml(campaign.status)}">${status}</span></td>
    <td class="num"><time datetime="${escapeHtml(campaign.updatedAt ?? '')}" title="${escapeHtml(campaign.updatedAt ?? '')}">${escapeHtml(relativeTime(campaign.updatedAt))}</time></td>`;
}

// Whether a campaign's rows are open: running campaigns start expanded so the
// live picture needs no click; anything can be toggled either way, and the
// choice survives every poll.
function isExpanded(campaign) {
  if (state.collapsed.has(campaign.key)) return false;
  return state.expanded.has(campaign.key) || campaign.status === 'running';
}

// One row per attempt, always available: the run-level facts an operator
// otherwise opens the modal and expands each attempt for. While an attempt
// runs, its phase, score, elapsed time and spend update on every poll.
function attemptRows(campaign) {
  const attempts = [...(campaign.attempts ?? [])]
    .sort((left, right) => STACK_ORDER.indexOf(left.stack) - STACK_ORDER.indexOf(right.stack)
      || (left.repetition ?? 1) - (right.repetition ?? 1));
  const rows = attempts.map(attempt => {
    const metrics = attemptMetrics(attempt);
    const reason = attemptExcluded(attempt);
    const running = attempt.status === 'running';
    const queued = attempt.status === 'pending';
    const first = metrics?.raw.first ? `${metrics.raw.first.score}/${metrics.raw.first.max}` : '—';
    const score = metrics?.raw.final ? `${metrics.raw.final.score}/${metrics.raw.final.max}`
      : attempt.progress?.latestScore
        ? `${attempt.progress.latestScore.score}/${attempt.progress.latestScore.max}` : '—';
    const level = attempt.progress?.level;
    const middle = reason ? `excluded — ${reason}`
      : running || queued ? (attempt.progress?.phase ?? '') : '';
    const duration = attempt.result?.durationSec != null
      ? durationFromSeconds(attempt.result.durationSec)
      : running ? elapsedTime(attempt.execution?.startedAt) : '—';
    return `<tr class="attempt-row">
      <th scope="row">${escapeHtml(title(attempt.stack))} <i>rep ${escapeHtml(attempt.repetition ?? 1)}</i></th>
      <td class="attempt-middle">${middle ? `<span>${escapeHtml(middle)}</span>`
        : `<span class="status ${escapeHtml(attempt.status)}">${escapeHtml(statusLabel(attempt.status))}</span>`}</td>
      <td class="num">${level ? `L${escapeHtml(level)}` : '—'}</td>
      <td class="num">${escapeHtml(first)}</td>
      <td class="num">${escapeHtml(score)}</td>
      <td class="num">${metrics ? metrics.rounds : (attempt.progress?.repair?.round || '—')}</td>
      <td class="num">${escapeHtml(duration)}</td>
      <td class="num">${money(attemptSpend(attempt))}</td>
    </tr>`;
  }).join('');
  return `<td colspan="3"><table class="attempt-grid">
    <thead><tr><th scope="col">Attempt</th><th scope="col">State</th>
      <th class="num" scope="col">Level</th><th class="num" scope="col">First</th>
      <th class="num" scope="col">Latest</th><th class="num" scope="col">Rounds</th>
      <th class="num" scope="col">Time</th><th class="num" scope="col">Cost</th></tr></thead>
    <tbody>${rows}</tbody>
  </table></td>`;
}

// Refresh updates rows in place instead of replacing the table, so an element
// under the pointer survives the poll and a click cannot land on a node that
// was just thrown away.
function reconcileRows(tbody, desired) {
  // Skeleton and placeholder rows carry no key and never survive a real render.
  for (const row of [...tbody.children]) if (!row.dataset.rowKey) row.remove();
  const existing = new Map([...tbody.children].map(row => [row.dataset.rowKey, row]));
  let cursor = null;
  for (const [key, entry] of desired) {
    let row = existing.get(key);
    if (!row) {
      row = document.createElement('tr');
      row.dataset.rowKey = key;
    }
    existing.delete(key);
    if (row.dataset.html !== entry.html) {
      row.innerHTML = entry.html;
      row.dataset.html = entry.html;
    }
    if (row.className !== entry.className) row.className = entry.className;
    if (entry.toggles) row.dataset.toggles = entry.toggles;
    else delete row.dataset.toggles;
    const expected = cursor ? cursor.nextElementSibling : tbody.firstElementChild;
    if (row !== expected) tbody.insertBefore(row, expected);
    cursor = row;
  }
  for (const row of existing.values()) row.remove();
}

// Running campaigns first, then the record by recency. History nobody acted on
// leaves the default view after ARCHIVE_AFTER_MS; the toggle brings it back.
function partitionCampaigns(campaigns) {
  const age = campaign => Date.now() - (Date.parse(campaign.updatedAt ?? '') || 0);
  const archived = campaign => campaign.status !== 'running' && campaign.status !== 'completed'
    && age(campaign) > ARCHIVE_AFTER_MS;
  const order = (left, right) =>
    String(right.updatedAt ?? '').localeCompare(String(left.updatedAt ?? ''));
  // Running campaigns live in the Now zone, not the history table.
  return {
    visible: campaigns.filter(campaign => campaign.status !== 'running'
      && (state.showArchived || !archived(campaign))).sort(order),
    archivedCount: campaigns.filter(archived).length,
  };
}

function renderHistory() {
  const { visible, archivedCount } = partitionCampaigns(state.overview.campaigns);
  const tbody = $('#campaign-list');
  if (!visible.length) {
    tbody.replaceChildren();
    tbody.innerHTML = '<tr><td colspan="3" class="vacant-row">No campaign history yet.</td></tr>';
  } else {
    const desired = new Map();
    for (const campaign of visible) {
      // The whole row toggles the attempt rows; the title button opens the
      // evidence modal and stays for keyboard and assistive access.
      desired.set(campaign.key, { html: campaignRow(campaign), className: '',
        toggles: campaign.key });
      if (isExpanded(campaign)) {
        desired.set(`${campaign.key}::attempts`, { html: attemptRows(campaign), className: 'live-row' });
      }
    }
    reconcileRows(tbody, desired);
  }
  $('#campaign-count').textContent = visible.length ? plural(visible.length, 'campaign') : '';
  const toggle = $('#toggle-archived');
  toggle.hidden = !archivedCount && !state.showArchived;
  toggle.textContent = state.showArchived
    ? 'Hide stale campaigns' : `Show ${plural(archivedCount, 'stale campaign')}`;
}

function render() {
  const overview = state.overview;
  renderNow();
  renderVerdict();
  renderHistory();
  const startable = overview.canStart && overview.plans.some(plan => plan.state === 'frozen');
  $('#open-run').disabled = !startable;
  $('#open-run').title = overview.canStart ? '' : 'Run controls are enabled in the Docker appliance.';
  $('#mode-tag').hidden = overview.canStart;
}

// --------------------------------------------------------------------- detail

function artifactUrl(campaign, artifact, download = false) {
  return `/api/campaigns/${encodeURIComponent(campaign.key)}/artifacts/${encodeURIComponent(artifact.id)}${download ? '?download=1' : ''}`;
}

function renderArtifactLinks(campaign, artifacts) {
  return artifacts.length ? `<div class="artifact-list">${artifacts.map(artifact => `
    <article class="artifact-row">
      <div><strong>${escapeHtml(artifact.name)}</strong><small>${escapeHtml(artifact.path)} · ${Math.max(1, Math.ceil(artifact.size / 1024))} KB</small></div>
      <div class="artifact-actions"><a href="${artifactUrl(campaign, artifact)}" target="_blank" rel="noopener">Open</a><a href="${artifactUrl(campaign, artifact, true)}" download>Download</a></div>
    </article>`).join('')}</div>` : '<p class="package-empty">No files have been written for this execution yet.</p>';
}

function renderVisuals(campaign, visuals) {
  return visuals.length ? `<div class="visual-grid">${visuals.map(visual => `
    <a href="${artifactUrl(campaign, visual)}" target="_blank" rel="noopener" title="Open ${escapeHtml(visual.name)}">
      <img src="${artifactUrl(campaign, visual)}" alt="${escapeHtml(visual.name)}" loading="lazy">
      <span>${escapeHtml(visual.name)}</span>
    </a>`).join('')}</div>` : '<p class="package-empty">No screenshots were captured for this execution.</p>';
}

function renderExecutionPackage(campaign, evidence) {
  const files = evidence.artifacts.filter(artifact => artifact.kind !== 'visual');
  return `<details class="evidence-package">
    <summary>Execution ${evidence.ordinal} package <span>${plural(evidence.visuals.length, 'screenshot')} · ${plural(evidence.artifacts.length, 'file')}${evidence.truncated ? ' · list limited' : ''}</span></summary>
    ${evidence.truncated ? '<p class="package-notice">This unusually large execution has more retained files than the dashboard lists. Its durable package is unchanged.</p>' : ''}
    <div class="package-section"><h4>Visual evidence</h4>${renderVisuals(campaign, evidence.visuals)}</div>
    <div class="package-section"><h4>Run files</h4>${renderArtifactLinks(campaign, files)}</div>
  </details>`;
}

// Where a score becomes explicable: what the first build scored, how many
// repairs it took, how long it ran, what that cost, and which checks were
// still failing.
function levelTable(attempt) {
  const levels = attempt.result?.levels ?? [];
  if (!levels.length) return '<p class="package-empty">No level has been graded yet.</p>';
  return `<div class="compare-wrap"><table class="levels">
    <thead><tr><th scope="col">Level</th><th scope="col" class="num">First try</th>
      <th scope="col" class="num">After repair</th><th scope="col" class="num">Repairs</th>
      <th scope="col" class="num">Time</th>
      <th scope="col" class="num">Cost</th><th scope="col">Still failing</th></tr></thead>
    <tbody>${levels.map(level => `<tr>
      <th scope="row">L${escapeHtml(level.level)}</th>
      <td class="num">${level.firstAbort
        ? `<span class="aborted" title="${escapeHtml(level.firstAbort.reason ?? '')}">aborted (${escapeHtml(level.firstAbort.phase)})</span>`
        : level.firstScore ? `${level.firstScore.score}/${level.firstScore.max}` : '—'}</td>
      <td class="num">${level.finalScore ? `${level.finalScore.score}/${level.finalScore.max}` : '—'}</td>
      <td class="num">${level.roundsUsed ?? 0}${level.repairStatus ? ` <i>${escapeHtml(level.repairStatus)}</i>` : ''}</td>
      <td class="num">${durationFromSeconds(level.durationSec)}</td>
      <td class="num">${money(level.costUsd)}</td>
      <td>${level.failures?.length
        ? `<ul class="failures">${level.failures.map(failure => `<li>${escapeHtml(failure)}</li>`).join('')}</ul>`
        : '<span class="muted">none</span>'}</td>
    </tr>`).join('')}</tbody>
  </table></div>`;
}


// ------------------------------------------------------------- questlines

// A questline reads as a journey: its stations in order, lit as far as the run
// got. The dependency edges stay in the engine and the tooltips — drawing all
// of them made the picture about the compiler instead of the product.
function questlineLanes(dependency) {
  const nodes = dependency.nodes ?? [];
  const byId = new Map(nodes.map(node => [node.id, node]));
  const depths = new Map();
  const depthOf = id => {
    if (depths.has(id)) return depths.get(id);
    depths.set(id, 0);
    const node = byId.get(id);
    const parents = (node?.dependencies ?? []).filter(parent => byId.has(parent));
    const value = parents.length ? 1 + Math.max(...parents.map(depthOf)) : 0;
    depths.set(id, value);
    return value;
  };
  nodes.forEach(node => depthOf(node.id));
  // the definition declares each questline's order; sorting by depth is only a
  // fallback for state written before the ordered lists were exposed
  return (dependency.questlines ?? []).map(questline => ({
    id: questline.id,
    title: questline.title,
    stations: Array.isArray(questline.nodes) && questline.nodes.length
      ? questline.nodes.map(id => byId.get(id)).filter(Boolean)
      : nodes.filter(node => node.questline === questline.id)
        .sort((a, b) => depthOf(a.id) - depthOf(b.id) || a.id.localeCompare(b.id)),
  })).filter(lane => lane.stations.length);
}

// exhausted is terminal — the strikes are gone — so it reads as red. A
// regression is a live fight, so it keeps the warning yellow.
const NODE_CLASS = { passed: 'pass', active: 'act', exhausted: 'fail', regressed: 'warn',
  blocked: 'off', locked: 'off' };

// Why a station is dark, in its own words: the first unmet prerequisite.
function stationTip(node, byId) {
  const state = statusLabel(node.status) ?? node.status;
  if (node.status === 'blocked' || node.status === 'locked') {
    const missing = (node.dependencies ?? []).map(id => byId.get(id))
      .find(parent => parent && parent.status !== 'passed');
    if (missing) return `${node.title} — waiting on ${missing.title}`;
  }
  return `${node.title} — ${state}`
    + (node.checks.total ? ` · ${node.checks.passed}/${node.checks.total} checks` : '');
}

function questlineColumn(lanes, dependency, layout, offsetX) {
  const byId = new Map((dependency.nodes ?? []).map(node => [node.id, node]));
  const scores = new Map((dependency.score?.questlines ?? [])
    .map(questline => [questline.id, questline]));
  const parts = lanes.map((lane, index) => {
    const y = layout.top + index * layout.laneH;
    const stations = lane.stations.map(node => byId.get(node.id) ?? node);
    const pieces = [];
    for (let i = 0; i < stations.length - 1; i += 1) {
      const lit = stations[i].status === 'passed' && stations[i + 1].status === 'passed';
      pieces.push(`<line class="ls${lit ? ' lit' : ''}" x1="${layout.x0 + i * layout.step}" y1="${y}" x2="${layout.x0 + (i + 1) * layout.step}" y2="${y}"/>`);
    }
    stations.forEach((node, i) => {
      pieces.push(`<g><circle class="cn ${NODE_CLASS[node.status] ?? 'off'}" cx="${layout.x0 + i * layout.step}" cy="${y}" r="4"/>`
        + `<title>${escapeHtml(stationTip(node, byId))}</title></g>`);
    });
    const score = scores.get(lane.id);
    const value = score?.percentage ?? (score?.availablePoints
      ? Math.round(100 * score.passedPoints / score.availablePoints) : null);
    pieces.push(`<text class="lq" x="${layout.width - 8}" y="${y + 3}" text-anchor="end">${value == null ? '—' : `${value}%`}</text>`);
    return pieces.join('');
  });
  return `<g transform="translate(${offsetX} 0)">${parts.join('')}</g>`;
}

// The same journeys transposed: one row per stack, every questline in one
// strip, so three stacks compare down a single page of three lines.
function questlineStrip(columns, lanes) {
  const LEFT = 118, STEP = 12, PAD = 5, GAP = 12, TOP = 58, ROWH = 34;
  const groups = [];
  let x = LEFT;
  for (const lane of lanes) {
    const width = PAD * 2 + (lane.stations.length - 1) * STEP;
    groups.push({ lane, x, width });
    x += width + GAP;
  }
  const statsX = x + 4;
  const width = statsX + 96;
  const height = TOP + columns.length * ROWH + 2;

  const headers = groups.map(group => {
    const short = group.lane.title.length > 15 ? `${group.lane.title.slice(0, 14)}…` : group.lane.title;
    return `<g><text class="gq" x="${group.x + 2}" y="${TOP - 14}" transform="rotate(-30 ${group.x + 2} ${TOP - 14})">${escapeHtml(short)}</text>`
      + `<title>${escapeHtml(group.lane.title)}</title></g>`
      + `<line class="gq-rule" x1="${group.x}" y1="${TOP - 8}" x2="${group.x + group.width}" y2="${TOP - 8}"/>`;
  }).join('');

  const rows = columns.map(({ stack, attempt }, index) => {
    const y = TOP + index * ROWH + 14;
    const byId = new Map((attempt.dependency.nodes ?? []).map(node => [node.id, node]));
    const pieces = [];
    for (const group of groups) {
      const stations = group.lane.stations.map(node => byId.get(node.id) ?? node);
      for (let i = 0; i < stations.length - 1; i += 1) {
        const lit = stations[i].status === 'passed' && stations[i + 1].status === 'passed';
        pieces.push(`<line class="ls${lit ? ' lit' : ''}" x1="${group.x + PAD + i * STEP}" y1="${y}" x2="${group.x + PAD + (i + 1) * STEP}" y2="${y}"/>`);
      }
      stations.forEach((node, i) => {
        pieces.push(`<g><circle class="cn ${NODE_CLASS[node.status] ?? 'off'}" cx="${group.x + PAD + i * STEP}" cy="${y}" r="3.4"/>`
          + `<title>${escapeHtml(`${group.lane.title}: ${stationTip(node, byId)}`)}</title></g>`);
      });
    }
    const score = attempt.dependency.score;
    const final = score?.status === 'final';
    const unique = score?.uniqueChecks;
    const value = final && score.averagePercentage != null ? Math.round(score.averagePercentage)
      : unique?.availablePoints ? Math.round(100 * unique.passedPoints / unique.availablePoints) : null;
    const spend = attemptSpend(attempt);
    return `<g>
      <text class="ch-name" x="${LEFT - 14}" y="${y - 4}" text-anchor="end">${escapeHtml(title(stack))}</text>
      <text class="ch-sub" x="${LEFT - 14}" y="${y + 9}" text-anchor="end">${spend == null ? '' : escapeHtml(money(spend))}</text>
      <text class="ch-score${final ? '' : ' prov'}" x="${statsX}" y="${y + 5}">${value == null ? '—' : `${value}%`}</text>
      ${pieces.join('')}
    </g>`;
  }).join('');
  return `<svg viewBox="0 0 ${width} ${height}" width="${width}" role="img"
    aria-label="one row per stack, every questline as a strip of stations">${headers}${rows}</svg>`;
}

// One stack with the panel to itself: the real dependency graph, every node
// named, every edge a declared prerequisite. Columns are dependency depth,
// bands are questlines, and a failed node visibly cuts the branch it blocks.
function questlineSingle(column, lanes) {
  const { attempt } = column;
  const nodes = attempt.dependency.nodes ?? [];
  const byId = new Map(nodes.map(node => [node.id, node]));
  const scores = new Map((attempt.dependency.score?.questlines ?? [])
    .map(questline => [questline.id, questline]));
  const depths = new Map();
  const depthOf = id => {
    if (depths.has(id)) return depths.get(id);
    depths.set(id, 0);
    const node = byId.get(id);
    const parents = (node?.dependencies ?? []).filter(parent => byId.has(parent));
    const value = parents.length ? 1 + Math.max(...parents.map(depthOf)) : 0;
    depths.set(id, value);
    return value;
  };
  nodes.forEach(node => depthOf(node.id));
  const maxDepth = Math.max(0, ...depths.values());

  // the columns spread to the panel rather than huddling at a fixed step —
  // without node labels the width is free, and longer edges cross less steeply
  const LABEL = 148, ROWH = 26, BANDPAD = 12, TOP = 56, WIDTH = 800;
  const STEP = Math.floor((WIDTH - LABEL - 20 - 40 - 54) / Math.max(1, maxDepth));
  const colX = depth => LABEL + 20 + depth * STEP;
  const width = WIDTH;

  // band layout: within a questline, stack nodes that share a depth
  const positions = new Map();
  let y = TOP;
  const bands = lanes.map(lane => {
    const slots = new Map();
    let rows = 1;
    for (const station of lane.stations) {
      const depth = depthOf(station.id);
      const slot = slots.get(depth) ?? 0;
      slots.set(depth, slot + 1);
      rows = Math.max(rows, slot + 1);
      positions.set(station.id, { x: colX(depth), y: y + 6 + slot * ROWH });
    }
    const band = { lane, top: y, rows };
    y += rows * ROWH + BANDPAD;
    return band;
  });
  const height = y - BANDPAD + 8;

  const score = attempt.dependency.score;
  const final = score?.status === 'final';
  const unique = score?.uniqueChecks;
  const value = final && score.averagePercentage != null ? Math.round(score.averagePercentage)
    : unique?.availablePoints ? Math.round(100 * unique.passedPoints / unique.availablePoints) : null;
  const spend = attemptSpend(attempt);
  const passed = nodes.filter(node => node.status === 'passed').length;
  const head = `<text class="ch-name" x="${LABEL}" y="16">${escapeHtml(title(column.stack))}</text>
    <text class="ch-score${final ? '' : ' prov'}" x="${LABEL}" y="38">${value == null ? '—' : `${value}%`}</text>
    <text class="ch-sub" x="${width - 10}" y="38" text-anchor="end">${escapeHtml(`${passed}/${nodes.length}${spend == null ? '' : ` · ${money(spend)}`}`)}</text>`;

  // edges under nodes: a lit edge joins two passes; a cut edge leaves a node
  // that conclusively failed, and carries the blockage story
  const edges = nodes.flatMap(node => (node.dependencies ?? [])
    .filter(parent => positions.has(parent) && positions.has(node.id)).map(parent => {
      const from = positions.get(parent), to = positions.get(node.id);
      const x1 = from.x + 6, y1 = from.y;
      const x2 = to.x - 6, y2 = to.y;
      const parentStatus = byId.get(parent)?.status;
      const cls = parentStatus === 'passed' && node.status === 'passed' ? ' lit'
        : parentStatus === 'exhausted' ? ' cut'
        : parentStatus === 'regressed' ? ' cut warn' : '';
      const bend = Math.max(14, (x2 - x1) / 2);
      return `<path class="de${cls}" d="M${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}"/>`;
    }));

  // dots and edges only — a label floating in a graph always finds something
  // to collide with, so the names of the broken nodes live below the drawing
  const pills = nodes.filter(node => positions.has(node.id)).map(node => {
    const point = positions.get(node.id);
    return `<g><circle class="cn ${NODE_CLASS[node.status] ?? 'off'}" cx="${point.x}" cy="${point.y}" r="5.5"/>`
      + `<title>${escapeHtml(stationTip(node, byId))}</title></g>`;
  });

  const sides = bands.map(band => {
    const laneY = band.top + 6 + ((band.rows - 1) * ROWH) / 2;
    const laneScore = scores.get(band.lane.id);
    const laneValue = laneScore?.percentage ?? (laneScore?.availablePoints
      ? Math.round(100 * laneScore.passedPoints / laneScore.availablePoints) : null);
    const words = band.lane.title.split(' ');
    const label = band.lane.title.length <= 20 || words.length < 2
      ? `<text class="cq" x="${LABEL - 14}" y="${laneY + 3}" text-anchor="end">${escapeHtml(band.lane.title)}</text>`
      : `<text class="cq" x="${LABEL - 14}" y="${laneY - 2}" text-anchor="end">${escapeHtml(words.slice(0, Math.ceil(words.length / 2)).join(' '))}</text>`
        + `<text class="cq" x="${LABEL - 14}" y="${laneY + 9}" text-anchor="end">${escapeHtml(words.slice(Math.ceil(words.length / 2)).join(' '))}</text>`;
    return label + `<text class="lq${laneValue === 100 ? ' full' : ''}" x="${width - 10}" y="${laneY + 3}" text-anchor="end">${laneValue == null ? '—' : `${laneValue}%`}</text>`;
  }).join('');

  // the graph's only prose: what is red and what is yellow, by full name
  const exhausted = nodes.filter(node => node.status === 'exhausted').map(node => node.title);
  const regressed = nodes.filter(node => node.status === 'regressed').map(node => node.title);
  const failLine = exhausted.length || regressed.length
    ? `<p class="fail-line">${exhausted.length ? `<b>Out of strikes:</b> ${escapeHtml(exhausted.join(', '))}` : ''}${exhausted.length && regressed.length ? ' · ' : ''}${regressed.length ? `<i>Regressed:</i> ${escapeHtml(regressed.join(', '))}` : ''}</p>`
    : '';
  return `<svg viewBox="0 0 ${width} ${height}" width="${width}" role="img"
    aria-label="${escapeHtml(`${title(column.stack)} dependency graph`)}">${head}${sides}${edges.join('')}${pills.join('')}</svg>${failLine}`;
}

// The section only appears on dependency-mode campaigns with recorded state.
// Every stack is drawn to the same rules; what differs is how far each journey
// got before the strikes ran out.
function dependencyConstellation(campaign) {
  if (campaign.mode !== 'dependency') return '';
  const columns = STACK_ORDER.flatMap(stack => {
    const best = (campaign.attempts ?? [])
      .filter(attempt => attempt.stack === stack && attempt.dependency?.nodes?.length)
      .sort((left, right) => (right.dependency.attempts?.total ?? 0)
        - (left.dependency.attempts?.total ?? 0))[0];
    return best ? [{ stack, attempt: best }] : [];
  });
  if (!columns.length) return '';
  const lanes = questlineLanes(columns[0].attempt.dependency);
  if (!lanes.length) return '';
  const maxStations = Math.max(...lanes.map(lane => lane.stations.length));
  const LABEL = 150;
  const layout = { x0: 14, step: 26, top: 56, laneH: 27,
    width: 14 + (maxStations - 1) * 26 + 14 + 40 };
  const width = LABEL + columns.length * (layout.width + 10);
  const height = layout.top + lanes.length * layout.laneH + 2;
  const labels = lanes.map((lane, index) => {
    const y = layout.top + index * layout.laneH;
    const words = lane.title.split(' ');
    if (lane.title.length <= 20 || words.length < 2) {
      return `<text class="cq" x="${LABEL - 14}" y="${y + 3}" text-anchor="end">${escapeHtml(lane.title)}</text>`;
    }
    const middle = Math.ceil(words.length / 2);
    return `<text class="cq" x="${LABEL - 14}" y="${y - 2}" text-anchor="end">${escapeHtml(words.slice(0, middle).join(' '))}</text>`
      + `<text class="cq" x="${LABEL - 14}" y="${y + 9}" text-anchor="end">${escapeHtml(words.slice(middle).join(' '))}</text>`;
  }).join('');
  const heads = columns.map(({ stack, attempt }, index) => {
    const x = LABEL + index * (layout.width + 10);
    const score = attempt.dependency.score;
    const final = score?.status === 'final';
    const unique = score?.uniqueChecks;
    const value = final && score.averagePercentage != null ? Math.round(score.averagePercentage)
      : unique?.availablePoints ? Math.round(100 * unique.passedPoints / unique.availablePoints) : null;
    const spend = attemptSpend(attempt);
    const passed = (attempt.dependency.nodes ?? []).filter(node => node.status === 'passed').length;
    const scoreText = value == null ? '—' : `${value}%`;
    return `<g transform="translate(${x} 0)">
      <text class="ch-name" x="12" y="15">${escapeHtml(title(stack))}</text>
      <text class="ch-score${final ? '' : ' prov'}" x="12" y="37">${scoreText}</text>
      <text class="ch-sub" x="${layout.width - 8}" y="37" text-anchor="end">${escapeHtml(`${passed}/${(attempt.dependency.nodes ?? []).length}${spend == null ? '' : ` · ${money(spend)}`}`)}</text>
    </g>`;
  }).join('');
  const graphs = columns.map(({ attempt }, index) =>
    questlineColumn(lanes, attempt.dependency, layout, LABEL + index * (layout.width + 10))).join('');
  const views = ['questlines', 'stacks', ...columns.map(column => column.stack)];
  const active = views.includes(state.questlineView) ? state.questlineView : 'questlines';
  const button = (view, label) =>
    `<button type="button" data-qlview="${escapeHtml(view)}"${active === view ? ' class="on"' : ''}>${escapeHtml(label)}</button>`;
  return `<section class="detail-block">
    <div class="ql-head"><h3>Questlines</h3>
      <div class="ql-toggle" role="group" aria-label="Questline view">
        ${button('questlines', 'By questline')}${button('stacks', 'By stack')}${columns.map(column => button(column.stack, title(column.stack))).join('')}
      </div></div>
    <div class="constellation" data-qlpanel="questlines"${active === 'questlines' ? '' : ' hidden'}><svg viewBox="0 0 ${width} ${height}" width="${width}" role="img"
      aria-label="each questline as a journey per stack: stations light as they pass">${labels}${heads}${graphs}</svg></div>
    <div class="constellation" data-qlpanel="stacks"${active === 'stacks' ? '' : ' hidden'}>${questlineStrip(columns, lanes)}</div>
    ${columns.map(column => `<div class="constellation" data-qlpanel="${escapeHtml(column.stack)}"${active === column.stack ? '' : ' hidden'}>${questlineSingle(column, lanes)}</div>`).join('')}
    <p class="constellation-note">Each row is one product journey, its features in unlock order. A filled dot passed every check; red ran out of strikes; yellow regressed after passing; a dim ring is waiting on a prerequisite — hover it to see which. The percentage is that journey's score, and the headline is their equal-weight average, grey while the run is still in flight.</p>
  </section>`;
}

function dependencyWork(attempt) {
  const progress = attempt.dependency;
  if (!progress) return '';
  if (progress.unreadable) {
    return `<p class="package-notice">Dependency progress cannot be read: ${escapeHtml(progress.unreadable)}</p>`;
  }
  const section = (label, nodes) => `<div class="dependency-group"><h4>${label}</h4>${nodes.length
    ? `<ul>${nodes.map(node => `<li><strong>${escapeHtml(node.title)}</strong><span>L${node.level} · ${node.checks.passed}/${node.checks.total} checks passed</span></li>`).join('')}</ul>`
    : '<p>None</p>'}</div>`;
  const score = progress.score?.averagePercentage ?? progress.score?.uniqueChecks?.percentage;
  return `<section class="dependency-progress">
    <div class="dependency-summary">
      <p><span class="tag">Current level</span><strong>L${escapeHtml(progress.level)}</strong></p>
      <p><span class="tag">Attempts</span><strong>${escapeHtml(progress.attempts.level)} at this level</strong></p>
      <p><span class="tag">Strikes</span><strong>${escapeHtml(progress.attempts.used)} of ${escapeHtml(progress.attempts.budget)}</strong></p>
      <p><span class="tag">Score</span><strong>${score == null ? 'In progress' : `${Math.round(score)}%`}</strong></p>
      <p><span class="tag">Evidence</span><strong>${plural(progress.evidence.length, 'graded attempt')}</strong></p>
    </div>
    <div class="dependency-groups">
      ${section('Current work', progress.work.current)}
      ${section('Passed', progress.work.passed)}
      ${section('Needs work', progress.work.failed)}
      ${section('Waiting', progress.work.locked)}
    </div>
  </section>`;
}

// The identity an operator otherwise opens plan.json for. Only facts the plan
// actually carries are shown; a schema-1 campaign shows what it has.
function factsBlock(campaign) {
  const facts = campaign.facts;
  if (!facts) return '';
  const digest = reference => {
    const match = /@sha256:([a-f0-9]{12})/.exec(String(reference ?? ''));
    return match ? `sha256:${match[1]}…` : reference ?? null;
  };
  const entries = [
    facts.mode ? ['Mode', title(facts.mode)] : null,
    ...facts.agents.map(agent => ['Agent', [agent.adapter, agent.version ? `@${agent.version}` : '',
      agent.model ? ` · ${agent.model}` : ''].join('')]),
    ...facts.recipes.filter(recipe => recipe.id).map(recipe =>
      [`L${recipe.level}`, `${recipe.id}@${recipe.version ?? '?'}`]),
    facts.runtime?.controllerImage ? ['Controller', digest(facts.runtime.controllerImage)] : null,
    facts.runtime?.buildImage ? ['Build image', digest(facts.runtime.buildImage)] : null,
    campaign.createdAt ? ['Started', new Date(campaign.createdAt).toLocaleString()] : null,
  ].filter(entry => entry && entry[1]);
  if (!entries.length) return '';
  return `<section class="detail-block"><div class="facts">${entries.map(([label, value]) =>
    `<p><span class="tag">${escapeHtml(label)}</span><strong class="mono">${escapeHtml(value)}</strong></p>`).join('')}</div></section>`;
}

// A collapsed attempt already answers the questions that matter: what it
// scored unaided, what repair reached, how many rounds and how long, and which
// checks are open. Phase text only appears while there is a phase to report.
function attemptSummaryLine(attempt) {
  const metrics = attemptMetrics(attempt);
  const reason = attemptExcluded(attempt);
  const queued = attempt.status === 'pending';
  const active = ['running', 'interrupted'].includes(attempt.status) || queued;
  const stillFailing = (attempt.result?.levels ?? []).flatMap(level => level.failures ?? []);
  const chips = stillFailing.slice(0, 3).map(failure =>
    `<span class="fail-chip" title="${escapeHtml(failure)}">${escapeHtml(failure.split('/').pop())}</span>`).join('')
    + (stillFailing.length > 3 ? `<span class="fail-chip more">+${stillFailing.length - 3}</span>` : '');
  const first = metrics?.raw.first ? `${metrics.raw.first.score}/${metrics.raw.first.max}` : '—';
  const final = metrics?.raw.final ? `${metrics.raw.final.score}/${metrics.raw.final.max}` : '—';
  const duration = attempt.result?.durationSec != null
    ? durationFromSeconds(attempt.result.durationSec)
    : attempt.status === 'running' ? elapsedTime(attempt.execution?.startedAt) : '—';
  return `<summary>
    <span class="attempt-name">${escapeHtml(title(attempt.stack))}<i>rep ${escapeHtml(attempt.repetition ?? 1)}</i></span>
    <span class="attempt-phase-inline">${active
    ? escapeHtml(reason || attempt.progress?.phase || '') : chips}</span>
    <span class="status ${escapeHtml(reason ? 'invalid' : attempt.status)}">${escapeHtml(reason ? 'Excluded' : statusLabel(attempt.status))}</span>
    <span class="attempt-figs">
      <b title="first build → after repair">${escapeHtml(first)} → ${escapeHtml(final)}</b>
      <i title="repair rounds">${metrics ? plural(metrics.rounds, 'round') : '—'}</i>
      <i title="duration">${escapeHtml(duration)}</i>
      <i title="normalized cost">${queued ? '—' : money(attemptSpend(attempt))}</i>
    </span>
  </summary>`;
}

async function showDetail(key) {
  state.openCampaign = key;
  if (location.hash !== `#campaign=${key}`) history.replaceState(null, '', `#campaign=${encodeURIComponent(key)}`);
  const dialog = $('#detail-dialog');
  $('#detail-title').textContent = 'Campaign';
  $('#detail-content').innerHTML = `<div class="loading-block">
    <span class="skeleton short"></span><span class="skeleton mid"></span>
    <span class="skeleton"></span><span class="skeleton mid"></span>
  </div>`;
  if (!dialog.open) dialog.showModal();
  try {
    const campaign = await request(`/api/campaigns/${encodeURIComponent(key)}`);
    if (state.openCampaign !== key) return;
    $('#detail-title').textContent = campaign.title;
    const graded = compareCampaign(campaign).usable.length;
    const summary = (campaign.statusReason
        ? `<p class="package-notice">${escapeHtml(campaign.statusReason)}</p>` : '')
      + (campaign.mode === 'dependency' && campaign.status === 'prepared'
        && (campaign.summary?.executions ?? 0) > 0
        ? `<p class="detail-actions"><button class="primary" type="button" data-resume="${escapeHtml(campaign.key)}">Resume campaign</button></p>` : '')
      + factsBlock(campaign)
      + dependencyConstellation(campaign)
      + (graded ? `<section class="detail-block"><h3>Comparison</h3>${comparisonTable(campaign)}</section>` : '');
    const files = campaign.package?.campaign ?? [];
    const campaignPackage = files.length ? `<section class="detail-block file-row">
      <span class="tag">Campaign files</span>
      ${files.map(file => `<a href="${artifactUrl(campaign, file)}" target="_blank" rel="noopener">${escapeHtml(file.name)}</a>`).join('')}
    </section>` : '';
    const attempts = [...campaign.attempts].sort((left, right) =>
      (left.repetition ?? 1) - (right.repetition ?? 1)
      || STACK_ORDER.indexOf(left.stack) - STACK_ORDER.indexOf(right.stack)).map(attempt => {
      const evidence = (campaign.package?.executions ?? []).filter(item => item.attemptId === attempt.id);
      const reason = attemptExcluded(attempt);
      return `<details class="attempt">
        ${attemptSummaryLine(attempt)}
        <div class="attempt-body">
          ${reason ? `<p class="package-notice">Excluded from the comparison — ${escapeHtml(reason)}</p>` : ''}
          ${dependencyWork(attempt)}
          ${levelTable(attempt)}
          ${evidence.map(item => renderExecutionPackage(campaign, item)).join('')
            || '<p class="package-empty">This attempt has not produced an execution package yet.</p>'}
          <details class="log-details"><summary>Recent run output</summary><pre>${escapeHtml(attempt.log || 'No output has been written yet.')}</pre></details>
        </div>
      </details>`;
    }).join('');
    $('#detail-content').innerHTML = summary
      + `<section class="detail-block"><h3>Attempts</h3><div class="attempt-list">${attempts}</div></section>`
      + campaignPackage;
  } catch (error) {
    $('#detail-content').innerHTML = `<p class="form-message">${escapeHtml(error.message)}</p>`;
  }
}

function closeDetail() {
  state.openCampaign = null;
  if (location.hash.startsWith('#campaign=')) history.replaceState(null, '', '#overview');
  $('#detail-dialog').close();
}

function openFromHash() {
  const match = /^#campaign=(.+)$/.exec(location.hash);
  if (match) showDetail(decodeURIComponent(match[1]));
  else if ($('#detail-dialog').open) closeDetail();
}

// ------------------------------------------------------------------- plumbing

function populatePlans() {
  let mode = $('#mode-select').value;
  const frozen = state.overview.plans.filter(plan => plan.state === 'frozen');
  if (!frozen.some(plan => plan.mode === mode) && frozen[0]?.mode) {
    mode = frozen[0].mode;
    $('#mode-select').value = mode;
  }
  const plans = frozen.filter(plan => plan.mode === mode);
  $('#plan-select').innerHTML = plans.map(plan => `<option value="${escapeHtml(plan.id)}">${escapeHtml(plan.title)}</option>`).join('');
  state.selectedPlan = plans[0] ?? null;
  renderPlanSummary();
}

function runName(plan) {
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace('T', '-').slice(0, 13);
  return `${plan.id}-${stamp}`.toLowerCase().replace(/[^a-z0-9.-]+/g, '-');
}

function renderPlanSummary() {
  const plan = state.selectedPlan;
  if (!plan) {
    $('#plan-summary').innerHTML = '<p class="package-empty">No frozen plans are available.</p>';
    return;
  }
  const facts = [
    ['Mode', title(plan.mode)],
    ['Stacks', plan.stacks.map(title).join(', ')],
    ['Levels', plan.levels.map(level => `L${level}`).join('–')],
    ['Attempts', plan.attempts],
    ['At once', plan.parallelism],
    ['Repair rounds', plan.budgets.fixRounds],
    ['Time limit', `${plan.budgets.attemptTimeoutMinutes} min`],
  ];
  $('#plan-summary').innerHTML = facts.map(([label, value]) =>
    `<p><span class="tag">${escapeHtml(label)}</span><strong class="mono">${escapeHtml(value)}</strong></p>`).join('');
  $('#output-name').value = runName(plan);
}

async function refresh({ quiet = false } = {}) {
  try {
    const overview = await request('/api/overview');
    state.overview = overview;
    state.csrfToken = overview.csrfToken;
    state.lastRefreshAt = Date.now();
    render();
    // Silence is the healthy state. A permanent "connected" light reports
    // nothing, so this only speaks when a refresh fails.
    $('#refresh-state').textContent = '';
  } catch (error) {
    $('#refresh-state').textContent = quiet ? 'Not updating' : error.message;
  }
}

// The liveness tick only appears while something is running; a static page
// claiming freshness every second is noise.
function renderTick() {
  const tick = $('#refresh-tick');
  const live = state.overview?.campaigns.some(campaign => campaign.status === 'running');
  if (!live || !state.lastRefreshAt) { tick.textContent = ''; return; }
  tick.textContent = `updated ${Math.max(0, Math.round((Date.now() - state.lastRefreshAt) / 1000))}s ago`;
}

document.addEventListener('click', event => {
  const resume = event.target.closest('[data-resume]');
  if (resume?.dataset.resume) {
    resume.disabled = true;
    controlRequest(`/api/campaigns/${encodeURIComponent(resume.dataset.resume)}/resume`, {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' },
    ).then(() => { closeDetail(); return refresh(); })
      .catch(error => { resume.disabled = false; resume.title = error.message; });
    return;
  }
  const detail = event.target.closest('[data-campaign]');
  if (detail?.dataset.campaign) { showDetail(detail.dataset.campaign); return; }
  const qlview = event.target.closest('[data-qlview]');
  if (qlview) {
    state.questlineView = qlview.dataset.qlview;
    document.querySelectorAll('[data-qlview]').forEach(button =>
      button.classList.toggle('on', button.dataset.qlview === state.questlineView));
    document.querySelectorAll('[data-qlpanel]').forEach(panel =>
      panel.hidden = panel.dataset.qlpanel !== state.questlineView);
    return;
  }
  const toggle = event.target.closest('[data-toggles]');
  if (toggle?.dataset.toggles) {
    const key = toggle.dataset.toggles;
    const campaign = state.overview?.campaigns.find(item => item.key === key);
    if (isExpanded(campaign ?? { key })) { state.expanded.delete(key); state.collapsed.add(key); }
    else { state.collapsed.delete(key); state.expanded.add(key); }
    renderHistory();
  }
});
$('#close-detail').addEventListener('click', closeDetail);
// A click on the dialog element itself is a click on the backdrop — the
// panel's own content sits inside .detail-shell.
$('#detail-dialog').addEventListener('click', event => {
  if (event.target === event.currentTarget) closeDetail();
});
$('#detail-dialog').addEventListener('cancel', () => { state.openCampaign = null;
  if (location.hash.startsWith('#campaign=')) history.replaceState(null, '', '#overview'); });
$('#toggle-archived').addEventListener('click', () => {
  state.showArchived = !state.showArchived;
  renderHistory();
});
window.addEventListener('hashchange', openFromHash);
$('#open-run').addEventListener('click', () => { populatePlans(); $('#run-message').textContent = ''; $('#run-dialog').showModal(); });
$('#mode-select').addEventListener('change', populatePlans);
$('#plan-select').addEventListener('change', event => {
  state.selectedPlan = state.overview.plans.find(plan => plan.id === event.target.value) ?? null;
  renderPlanSummary();
});
$('#start-run').addEventListener('click', async () => {
  const message = $('#run-message');
  message.textContent = '';
  $('#start-run').disabled = true;
  try {
    await controlRequest('/api/campaigns', { method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ planId: $('#plan-select').value,
      outputName: $('#output-name').value }) });
    $('#run-dialog').close();
    await refresh();
  } catch (error) { message.textContent = error.message; }
  finally { $('#start-run').disabled = false; }
});

$('#campaign-list').innerHTML = skeletonRows(6);
await refresh();
openFromHash();
setInterval(() => refresh({ quiet: true }), 5000);
setInterval(renderTick, 1000);
