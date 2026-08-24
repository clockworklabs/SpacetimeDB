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

const state = { overview: null, csrfToken: null, selectedPlan: null, showArchived: false,
  openCampaign: null, expanded: new Set(), collapsed: new Set() };

const $ = selector => document.querySelector(selector);
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, character => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[character]);
const title = value => ({ postgres: 'PostgreSQL', mongodb: 'MongoDB', spacetime: 'SpacetimeDB' }[value] ?? value);
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
  <td class="band num"><span class="skeleton num"></span></td>
  <td class="band num"><span class="skeleton num"></span></td>
  <td class="band num"><span class="skeleton num"></span></td>
  <td><span class="skeleton short"></span></td>
  <td class="num"><span class="skeleton num"></span></td>
</tr>`).join('');

async function request(path, options) {
  const response = await fetch(path, options);
  const value = await response.json();
  if (!response.ok) throw new Error(value.error ?? `Request failed (${response.status})`);
  return value;
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

// The landing figure is the primary metric first: what each stack scored
// unaided, then what repair reached, then the median spend. Cost stops being
// shown as a comparison when the campaign's own detail view would refuse to
// compare it.
function stackCell(summary, stack, running) {
  const row = summary.usable.find(item => item.stack === stack);
  const burn = summary.burn.get(stack);
  if (!row) {
    if (running && burn != null) {
      return `<td class="band num partial"><b>…</b><small>${money(burn)} so far</small></td>`;
    }
    return burn != null
      ? `<td class="band num partial"><b>—</b><small>${money(burn)} spent</small></td>`
      : '<td class="band num vacant"><b>—</b></td>';
  }
  const cost = row.spend == null ? '' : summary.comparable
    ? `<small>${money(row.spend)}</small>`
    : `<small class="incomparable" title="Stacks graded different amounts of work; costs are not comparable">${money(row.spend)}*</small>`;
  return `<td class="band num">
    <b>${percent(row.first)}</b><i class="arrow">→ ${percent(row.final)}</i>
    ${cost || '<small class="vacant-small">no cost</small>'}
  </td>`;
}

function campaignRow(campaign) {
  const summary = compareCampaign(campaign);
  const running = campaign.status === 'running';
  const live = (campaign.attempts ?? []).filter(attempt =>
    ['running', 'pending'].includes(attempt.status)).length;
  const status = running ? `${live} of ${campaign.attempts.length} running`
    : escapeHtml(statusLabel(campaign.status));
  return `<td class="campaign-cell">
      <span class="chevron${isExpanded(campaign) ? ' open' : ''}" aria-hidden="true"></span>
      <div class="campaign-title">
        <button data-campaign="${escapeHtml(campaign.key)}">${escapeHtml(campaign.title)}</button>
        <i>${escapeHtml(campaign.key)}</i>
        ${campaign.error ? `<i class="row-error">${escapeHtml(campaign.error)}</i>` : ''}
      </div>
    </td>
    ${STACK_ORDER.map(stack => stackCell(summary, stack, running)).join('')}
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
  return `<td colspan="6"><table class="attempt-grid">
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
  const order = (left, right) => (right.status === 'running') - (left.status === 'running')
    || String(right.updatedAt ?? '').localeCompare(String(left.updatedAt ?? ''));
  return {
    visible: campaigns.filter(campaign => state.showArchived || !archived(campaign)).sort(order),
    archivedCount: campaigns.filter(archived).length,
  };
}

function renderHistory() {
  const { visible, archivedCount } = partitionCampaigns(state.overview.campaigns);
  const tbody = $('#campaign-list');
  if (!visible.length) {
    tbody.replaceChildren();
    tbody.innerHTML = '<tr><td colspan="6" class="vacant-row">No campaign history yet.</td></tr>';
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
      + factsBlock(campaign)
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
  const plans = state.overview.plans.filter(plan => plan.state === 'frozen');
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
    render();
    // Silence is the healthy state. A permanent "connected" light reports
    // nothing, so this only speaks when a refresh fails.
    $('#refresh-state').textContent = '';
  } catch (error) {
    $('#refresh-state').textContent = quiet ? 'Not updating' : error.message;
  }
}

document.addEventListener('click', event => {
  const detail = event.target.closest('[data-campaign]');
  if (detail?.dataset.campaign) { showDetail(detail.dataset.campaign); return; }
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
$('#detail-dialog').addEventListener('cancel', () => { state.openCampaign = null;
  if (location.hash.startsWith('#campaign=')) history.replaceState(null, '', '#overview'); });
$('#toggle-archived').addEventListener('click', () => {
  state.showArchived = !state.showArchived;
  renderHistory();
});
window.addEventListener('hashchange', openFromHash);
$('#open-run').addEventListener('click', () => { populatePlans(); $('#run-message').textContent = ''; $('#run-dialog').showModal(); });
$('#plan-select').addEventListener('change', event => {
  state.selectedPlan = state.overview.plans.find(plan => plan.id === event.target.value) ?? null;
  renderPlanSummary();
});
$('#start-run').addEventListener('click', async () => {
  const message = $('#run-message');
  message.textContent = '';
  $('#start-run').disabled = true;
  try {
    await request('/api/campaigns', { method: 'POST', headers: { 'content-type': 'application/json',
      'x-stack-bench-token': state.csrfToken }, body: JSON.stringify({ planId: $('#plan-select').value,
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
