const state = { overview: null, csrfToken: null, selectedPlan: null };

const $ = selector => document.querySelector(selector);
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, character => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[character]);
const title = value => ({ postgres: 'PostgreSQL', mongodb: 'MongoDB', spacetime: 'SpacetimeDB' }[value] ?? value);
const statusLabel = value => ({ running: 'Running', completed: 'Completed', prepared: 'Ready',
  'attention-required': 'Needs attention', invalid: 'Invalid run', unreadable: 'Cannot read' }[value] ?? value);
const relativeTime = value => {
  if (!value) return '—';
  const seconds = Math.max(0, (Date.now() - Date.parse(value)) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return new Date(value).toLocaleDateString();
};

async function request(path, options) {
  const response = await fetch(path, options);
  const value = await response.json();
  if (!response.ok) throw new Error(value.error ?? `Request failed (${response.status})`);
  return value;
}

function scoreHtml(score, label = 'latest score') {
  if (!score) return '<div class="score-line"><strong>—</strong><span>Waiting for a grade</span></div><progress max="1" value="0"></progress>';
  return `<div class="score-line"><strong>${score.score}/${score.max}</strong><span>${escapeHtml(label)}</span></div><progress max="${score.max}" value="${score.score}" aria-label="${escapeHtml(label)}"></progress>`;
}

function renderAttempt(attempt, campaign) {
  const score = attempt.progress.latestScore ?? attempt.result?.score;
  return `<article class="run-card">
    <button data-campaign="${escapeHtml(campaign.key)}" aria-label="Open ${escapeHtml(title(attempt.stack))} run detail">
      <div class="run-top"><span class="stack-name">${escapeHtml(title(attempt.stack))}</span><span class="status ${escapeHtml(attempt.status)}">${escapeHtml(statusLabel(attempt.status))}</span></div>
      <p class="phase">${escapeHtml(attempt.progress.phase)}</p>
      ${scoreHtml(score)}
      <div class="run-meta"><span>${escapeHtml(attempt.model)}</span><span>${escapeHtml(campaign.title)}</span></div>
    </button>
  </article>`;
}

function render() {
  const overview = state.overview;
  for (const key of ['running', 'completed', 'attention', 'prepared']) $(`#count-${key}`).textContent = overview.counts[key];
  const active = overview.campaigns.flatMap(campaign => campaign.attempts
    .filter(attempt => attempt.status === 'running').map(attempt => ({ attempt, campaign })));
  $('#run-grid').innerHTML = active.length ? active.map(({ attempt, campaign }) => renderAttempt(attempt, campaign)).join('')
    : '<div class="empty">No campaigns are running right now.</div>';
  $('#campaign-list').innerHTML = overview.campaigns.length ? overview.campaigns.map(campaign => `
    <article class="campaign-row">
      <div><button data-campaign="${escapeHtml(campaign.key)}">${escapeHtml(campaign.title)}</button><small>${escapeHtml(campaign.stacks?.map(title).join(' · ') || campaign.error || '')}</small></div>
      <span class="status ${escapeHtml(campaign.status)}">${escapeHtml(statusLabel(campaign.status))}</span>
      <small>${escapeHtml(relativeTime(campaign.updatedAt))}</small>
    </article>`).join('') : '<div class="empty">No campaign history yet.</div>';
  $('#operation-list').innerHTML = overview.operations.length ? overview.operations.slice(0, 8).map(operation => `
    <article class="operation-row"><strong>${escapeHtml(operation.type === 'campaign.run' ? 'Campaign launch' : operation.type)}</strong>
      <small>${escapeHtml(operation.outputName ?? operation.campaignId)} · ${escapeHtml(statusLabel(operation.status))} · ${escapeHtml(relativeTime(operation.updatedAt))}</small></article>`).join('')
    : '<div class="empty">No dashboard actions yet.</div>';
  $('#open-run').disabled = !overview.canStart || !overview.plans.some(plan => plan.state === 'frozen');
  $('#open-run').title = overview.canStart ? '' : 'Run controls are enabled in the Docker appliance.';
}

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
  $('#plan-summary').innerHTML = plan ? `${escapeHtml(plan.stacks.map(title).join(' + '))}<br>${escapeHtml(`L${plan.levels.join(' → L')} · ${plan.attempts} attempts · up to ${plan.parallelism} at once`)}<br>${escapeHtml(`${plan.budgets.fixRounds} repair rounds · ${plan.budgets.attemptTimeoutMinutes} minute limit`)}` : 'No frozen plans are available.';
  if (plan) $('#output-name').value = runName(plan);
}

async function refresh({ quiet = false } = {}) {
  try {
    const overview = await request('/api/overview');
    state.overview = overview;
    state.csrfToken = overview.csrfToken;
    render();
    $('#refresh-state').textContent = `Updated ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
    $('.live-dot').classList.remove('offline');
  } catch (error) {
    $('#refresh-state').textContent = quiet ? 'Update failed' : error.message;
    $('.live-dot').classList.add('offline');
  }
}

async function showDetail(key) {
  const dialog = $('#detail-dialog');
  $('#detail-title').textContent = 'Loading…';
  $('#detail-content').innerHTML = '';
  dialog.showModal();
  try {
    const campaign = await request(`/api/campaigns/${encodeURIComponent(key)}`);
    $('#detail-title').textContent = campaign.title;
    $('#detail-content').innerHTML = campaign.attempts.map(attempt => {
      const score = attempt.progress.latestScore ?? attempt.result?.score;
      return `<section class="attempt-detail">
        <header><div><h3>${escapeHtml(title(attempt.stack))}</h3><small>${escapeHtml(attempt.id)}</small></div><span class="status ${escapeHtml(attempt.status)}">${escapeHtml(statusLabel(attempt.status))}</span></header>
        <div class="attempt-facts">
          <div class="fact"><span>Current step</span><strong>${escapeHtml(attempt.progress.phase)}</strong></div>
          <div class="fact"><span>Latest score</span><strong>${score ? `${score.score}/${score.max}` : 'Waiting'}</strong></div>
          <div class="fact"><span>First build</span><strong>${attempt.progress.firstScore ? `${attempt.progress.firstScore.score}/${attempt.progress.firstScore.max}` : 'Waiting'}</strong></div>
          <div class="fact"><span>Started</span><strong>${escapeHtml(relativeTime(attempt.execution?.startedAt))}</strong></div>
        </div>
        <details><summary>Recent run output</summary><pre>${escapeHtml(attempt.log || 'No output has been written yet.')}</pre></details>
      </section>`;
    }).join('');
  } catch (error) {
    $('#detail-content').innerHTML = `<p class="form-message">${escapeHtml(error.message)}</p>`;
  }
}

document.addEventListener('click', event => {
  const target = event.target.closest('[data-campaign]');
  if (target) showDetail(target.dataset.campaign);
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

await refresh();
setInterval(() => refresh({ quiet: true }), 5000);
