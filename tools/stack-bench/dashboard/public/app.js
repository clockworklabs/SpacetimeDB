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

const state = { overview: null, csrfToken: null, selectedPlan: null };

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
// model got right unaided, and what the whole correction cost.
function attemptMetrics(attempt) {
  const run = attempt.result;
  if (!run || run.unreadable) return null;
  const levels = (run.levels ?? []).filter(level => level.finalScore);
  if (!levels.length) return null;
  const sum = (list, pick) => list.reduce((total, item) => total + pick(item), 0);
  const scored = levels.filter(level => level.firstScore);
  const firstMax = sum(scored, level => level.firstScore.max);
  const finalMax = sum(levels, level => level.finalScore.max);
  return {
    first: firstMax ? sum(scored, level => level.firstScore.score) / firstMax : null,
    final: sum(levels, level => level.finalScore.score) / finalMax,
    rounds: sum(levels, level => level.roundsUsed ?? 0),
    spend: attemptSpend(attempt),
    levelsGraded: levels.length,
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
      ?? { stack: attempt.stack, runs: [], excluded: [], pending: 0, spendSoFar: null };
    byStack.set(attempt.stack, entry);
    // Every attempt's spend counts toward burn, including ones excluded from
    // the result: a contaminated run still cost money.
    const incurred = attemptSpend(attempt);
    if (incurred != null) entry.spendSoFar = (entry.spendSoFar ?? 0) + incurred;
    const reason = attemptExcluded(attempt);
    if (reason) { entry.excluded.push({ attempt, reason }); continue; }
    const metrics = attempt.status === 'completed' ? attemptMetrics(attempt) : null;
    if (metrics) entry.runs.push({ attempt, metrics }); else entry.pending += 1;
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
  const summary = compareCampaign(campaign);
  const rows = new Map(summary.usable.map(row => [row.stack, row]));
  const cheapest = summary.comparable
    ? [...summary.priced].sort((left, right) => left.spend - right.spend)[0] : null;
  const cost = stack => {
    const row = rows.get(stack);
    const partial = summary.burn.get(stack);
    if (!row) {
      return partial != null
        ? `<td class="band num partial"><b>${money(partial)}</b></td>`
        : '<td class="band num vacant"><b>·</b></td>';
    }
    if (row.spend == null) return '<td class="band num vacant"><b>no cost</b></td>';
    return `<td class="band num${row === cheapest ? ' lead' : ''}"><b>${money(row.spend)}</b></td>`;
  };
  const live = (campaign.attempts ?? []).filter(attempt => attempt.status === 'running').length;
  const state = campaign.status === 'running' ? 'live'
    : ['attention-required', 'unreadable'].includes(campaign.status) ? 'flagged' : '';
  return `<tr class="${state}">
    <td class="campaign-cell">
      <button data-campaign="${escapeHtml(campaign.key)}">${escapeHtml(campaign.title)}</button>
      ${campaign.error ? `<i>${escapeHtml(campaign.error)}</i>` : ''}
    </td>
    ${STACK_ORDER.map(cost).join('')}
    <td class="state-cell">${live ? `${live} of ${campaign.attempts.length} running`
      : campaign.status === 'completed' ? '' : escapeHtml(statusLabel(campaign.status))}</td>
    <td class="num"><time datetime="${escapeHtml(campaign.updatedAt ?? '')}">${escapeHtml(relativeTime(campaign.updatedAt))}</time></td>
  </tr>`;
}

function renderHistory() {
  const campaigns = state.overview.campaigns;
  $('#campaign-list').innerHTML = campaigns.length
    ? campaigns.map(campaignRow).join('')
    : '<tr><td colspan="6" class="vacant-row">No campaign history yet.</td></tr>';
  $('#campaign-count').textContent = campaigns.length
    ? `${plural(campaigns.length, 'campaign')}` : '';
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
// repairs it took, what that cost, and which checks were still failing.
function levelTable(attempt) {
  const levels = attempt.result?.levels ?? [];
  if (!levels.length) return '<p class="package-empty">No level has been graded yet.</p>';
  return `<div class="compare-wrap"><table class="levels">
    <thead><tr><th scope="col">Level</th><th scope="col" class="num">First try</th>
      <th scope="col" class="num">After repair</th><th scope="col" class="num">Repairs</th>
      <th scope="col" class="num">Cost</th><th scope="col">Still failing</th></tr></thead>
    <tbody>${levels.map(level => `<tr>
      <th scope="row">L${escapeHtml(level.level)}</th>
      <td class="num">${level.firstScore ? `${level.firstScore.score}/${level.firstScore.max}` : '—'}</td>
      <td class="num">${level.finalScore ? `${level.finalScore.score}/${level.finalScore.max}` : '—'}</td>
      <td class="num">${level.roundsUsed ?? 0}${level.repairStatus ? ` <i>${escapeHtml(level.repairStatus)}</i>` : ''}</td>
      <td class="num">${money(level.costUsd)}</td>
      <td>${level.failures?.length
        ? `<ul class="failures">${level.failures.map(failure => `<li>${escapeHtml(failure)}</li>`).join('')}</ul>`
        : '<span class="muted">none</span>'}</td>
    </tr>`).join('')}</tbody>
  </table></div>`;
}

async function showDetail(key) {
  const dialog = $('#detail-dialog');
  $('#detail-title').textContent = 'Campaign';
  $('#detail-content').innerHTML = `<div class="loading-block">
    <span class="skeleton short"></span><span class="skeleton mid"></span>
    <span class="skeleton"></span><span class="skeleton mid"></span>
  </div>`;
  dialog.showModal();
  try {
    const campaign = await request(`/api/campaigns/${encodeURIComponent(key)}`);
    $('#detail-title').textContent = campaign.title;
    const graded = compareCampaign(campaign).usable.length;
    const summary = (campaign.statusReason
        ? `<p class="package-notice">${escapeHtml(campaign.statusReason)}</p>` : '')
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
      const metrics = attemptMetrics(attempt);
      // A queued attempt's figures belong to an earlier execution, so they are
      // not shown against this one.
      const queued = attempt.status === 'pending';
      const grade = !queued && metrics?.raw.final
        ? `${metrics.raw.final.score}/${metrics.raw.final.max}` : '—';
      return `<details class="attempt">
        <summary>
          <span class="attempt-name">${escapeHtml(title(attempt.stack))}<i>rep ${escapeHtml(attempt.repetition ?? 1)}</i></span>
          <span class="attempt-phase-inline">${escapeHtml(reason || (queued ? 'Waiting to start' : (attempt.progress?.phase ?? '')))}</span>
          <span class="status ${escapeHtml(reason ? 'invalid' : attempt.status)}">${escapeHtml(reason ? 'Excluded' : statusLabel(attempt.status))}</span>
          <span class="attempt-figs"><b>${escapeHtml(grade)}</b><i>${queued ? '—' : money(attemptSpend(attempt))}</i></span>
        </summary>
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
  const target = event.target.closest('[data-campaign]');
  if (target?.dataset.campaign) showDetail(target.dataset.campaign);
});
$('#close-detail').addEventListener('click', () => $('#detail-dialog').close());
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
setInterval(() => refresh({ quiet: true }), 5000);
