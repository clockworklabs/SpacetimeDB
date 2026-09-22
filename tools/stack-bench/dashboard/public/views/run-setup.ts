import type { RunSetupCatalog, RunSetupRequest, RunSetupReview } from '../../../src/campaigns/run-setup.js';
import { esc, money, modelLabel, stackLabel } from '../format.js';
import { runName } from './plans.js';

const guidanceLabel = (id: string) => ({ neutral: 'Standard skills',
  'neutral-no-sdk': 'No SDK skills or dev workflow',
  'neutral-dev': 'Standard skills + dev workflow',
  'neutral-dev-no-sdk': 'Dev workflow without SDK skills' } as Record<string, string>)[id] ?? id;

export function initialRun(catalog: RunSetupCatalog, id?: string): RunSetupRequest | null {
  const w = catalog.workloads.find(w => w.id === id)
    ?? catalog.workloads.find(w => w.mode === 'dependency' && w.workSelection === 'progressive')
    ?? catalog.workloads[0];
  if (!w) return null;
  return { key: runName(w.track, new Date()) + '-' + crypto.randomUUID().slice(0, 8),
    workload: w.id, workloadSha256: w.sha256, level: Math.max(...w.levels), stacks: [...w.stacks],
    agents: [{ index: 0, effort: w.agents[0]!.effort ?? 'medium' }],
    conditions: [(w.conditions.find(c => c.guidance === 'neutral-dev')
      ?? w.conditions.find(c => c.guidance === 'neutral') ?? w.conditions[0])!.id], ...w.defaults,
    productionQuality: true, maxCostUsd: w.defaults.maxCostUsd ?? 0, credentials: {} };
}

export function selectGuidance(conditions: RunSetupCatalog['workloads'][number]['conditions'], sdk: string, dev: string): string[] {
  return conditions.filter(c => c.sdkSkills === (sdk === 'on')
    && c.devWorkflow === (dev === 'on')).map(c => c.id);
}

export function readRunForm(form: HTMLFormElement, catalog: RunSetupCatalog): RunSetupRequest {
  const data = new FormData(form);
  return { key: String(data.get('key')), workload: String(data.get('workload')), workloadSha256: String(data.get('workloadSha256')),
    level: Number(data.get('level')), stacks: data.getAll('stack').map(String),
    agents: data.getAll('agent').map(index => ({ index: Number(index),
      effort: String(data.get(`effort-${index}`)) as RunSetupRequest['agents'][number]['effort'] })),
    conditions: data.has('sdkSkills') ? selectGuidance(catalog.workloads.find(w => w.id === data.get('workload'))!.conditions,
      String(data.get('sdkSkills')), String(data.get('devWorkflow'))) : data.getAll('condition').map(String), repetitions: Number(data.get('repetitions')),
    productionQuality: data.has('productionQuality'),
    parallelism: Number(data.get('parallelism')), repairs: Number(data.get('repairs')),
    timeoutMinutes: Number(data.get('timeoutMinutes')), maxCostUsd: Number(data.get('maxCostUsd')),
    pauseAfterDepth: data.get('pauseAfterDepth') ? Number(data.get('pauseAfterDepth')) : null,
    credentials: { adapters: Object.fromEntries([...data].filter(([key, value]) =>
      key.startsWith('credential-') && value).map(([key, value]) => [key.slice(11), String(value)])) } };
}

export function runSetupPage(catalog: RunSetupCatalog | null, request: RunSetupRequest | null,
  review: RunSetupReview | null, error: string, canStart: boolean): string {
  const field = (label: string, input: string) => `<label><span>${label}</span>${input}</label>`;
  const option = (value: string | number, label: string, selected: boolean) =>
    `<option value="${esc(String(value))}"${selected ? ' selected' : ''}>${esc(label)}</option>`;
  const integer = (name: string, value: number, min = 1) => `<input name="${name}" type="number" min="${min}" step="1" value="${value}" required>`;
  const alert = error ? `<p class="err" role="alert">${esc(error)}</p>` : '';
  const head = '<div class="page setup"><div class="title"><h2>New run</h2></div>';
  if (!catalog) return head + '<p role="status">Loading setup…</p>' + alert + '</div>';
  if (!request || !catalog.workloads.length) return head + '<p>No runnable workloads are configured. Run appliance setup to install the workload presets.</p>'
    + catalog.errors.map(error => `<p class="err">${esc(error)}</p>`).join('') + '</div>';
  const w = catalog.workloads.find(w => w.id === request.workload)!;
  const delivery = w.mode === 'dependency'
    ? ({ progressive: 'Progressive dependency graph', feature: 'One ready feature at a time',
      'all-at-once': 'Full graph in one build' }[w.workSelection as string] ?? w.workSelection)
    : 'Sequential levels';
  const splitGuidance = w.conditions.length === 4
    && new Set(w.conditions.map(c => `${c.sdkSkills}:${c.devWorkflow}`)).size === 4;
  const guidanceChoice = (key: 'sdkSkills' | 'devWorkflow', label: string) => {
    const value = w.conditions.find(c => request.conditions.includes(c.id))?.[key] ? 'on' : 'off';
    return field(label, `<select name="${key}">${[['on', 'On'], ['off', 'Off']]
      .map(([id, text]) => option(id!, text!, id === value)).join('')}</select>`);
  };
  const model = (index: number) => w.agents[index]!;
  if (review) {
    const rows = [
      ['Workload', `${w.title} · L${request.level}`],
      ['Work delivery', delivery],
      ['Stacks', request.stacks.map(stackLabel).join(', ')],
      ['Models', request.agents.map(a => `${modelLabel(model(a.index).model)} (${a.effort})`).join(', ')],
      ['Guidance', request.conditions.map(id => guidanceLabel(w.conditions.find(c => c.id === id)!.guidance)).join(', ')],
      ['Production-quality app', request.productionQuality ? 'Requested' : 'Not requested'],
      ['Runs', `${review.attempts} attempts · ${request.repetitions} per combination · ${review.parallelism} concurrent`],
      ['Repairs', `${request.repairs} per attempt`],
      ['Limits', `${request.timeoutMinutes} minutes and ${money(request.maxCostUsd)} per attempt`],
      ['Total cost cap', money(review.maxCostUsd)],
      ['Pause', request.pauseAfterDepth ? `After L${request.pauseAfterDepth}` : 'None'],
      ['Account', review.authentication.map(a => `${a.adapter}: ${a.profile ? `${a.profile.id} (${a.profile.mode})` : `appliance default (${a.source})`}`).join(', ')],
    ];
    return head + '<h3>Review run</h3><dl class="setup-review">' + rows.map(([key, value]) =>
      `<dt>${esc(key!)}</dt><dd>${esc(value!)}</dd>`).join('') + '</dl>'
      + (review.qualification === 'pending' ? '<p class="warning">Grading qualification is pending. Results will be provisional.</p>' : '')
      + '<p class="summary-note">Cost caps use recorded token pricing. Subscription usage is not an invoice charge.</p>'
      + `<details><summary>Recorded pricing and runtime</summary><pre>${esc(JSON.stringify({ pricing: review.pricing, runtime: review.runtime }, null, 2))}</pre></details>`
      + `<form data-run="setup-start" class="setup-actions"><button type="button" class="btn" data-setup-edit>Edit</button>`
      + '<button class="btn primary" type="submit">Start run</button></form>' + alert + '</div>';
  }
  return head + (canStart ? '' : '<p class="warning">This dashboard is read-only. Start the appliance to run a study.</p>')
    + `<form data-run="setup-review" class="setup-form"><input type="hidden" name="workloadSha256" value="${esc(request.workloadSha256)}">`
    + '<div class="setup-fields">'
    + field('Workload', `<select name="workload">${catalog.workloads.map(a => option(a.id, a.title, a.id === w.id)).join('')}</select>`)
    + field('Target', `<select name="level">${w.levels.map(n => option(n, `L${n}`, n === request.level)).join('')}</select>`)
    + `<p>${esc(String(delivery))}</p>`
    + '</div><fieldset><legend>Stacks</legend><div class="setup-choices">'
    + w.stacks.map(id => `<label><input type="checkbox" name="stack" value="${esc(id)}"${request.stacks.includes(id) ? ' checked' : ''}>${esc(stackLabel(id))}</label>`).join('')
    + '</div></fieldset><fieldset><legend>Models and reasoning</legend>'
    + w.agents.map((agent, index) => `<div class="setup-model"><label><input type="checkbox" name="agent" value="${index}"${request.agents.some(a => a.index === index) ? ' checked' : ''}>${esc(modelLabel(agent.model))}</label>`
      + `<select name="effort-${index}" aria-label="Reasoning for ${esc(agent.model)}">${['low', 'medium', 'high', 'xhigh', 'max'].map(e => option(e, e, e === (request.agents.find(a => a.index === index)?.effort ?? agent.effort ?? 'medium'))).join('')}</select></div>`).join('')
    + '</fieldset><fieldset><legend>App requirement</legend>'
    + `<label><input type="checkbox" name="productionQuality"${request.productionQuality ? ' checked' : ''}>Production-quality app</label>`
    + '<p>Build a production-quality application suitable for real users, not a prototype or demo.</p>'
    + '</fieldset><fieldset><legend>SpacetimeDB guidance</legend>'
    + (splitGuidance ? '<div class="setup-fields">' + guidanceChoice('sdkSkills', 'SDK skills')
      + guidanceChoice('devWorkflow', 'Dev workflow') : '<div class="setup-choices">' + w.conditions.map(c => `<label><input type="checkbox" name="condition" value="${esc(c.id)}"${request.conditions.includes(c.id) ? ' checked' : ''}>${esc(guidanceLabel(c.guidance))}</label>`).join(''))
    + '</div></fieldset><div class="setup-fields">'
    + field('Repetitions per combination', integer('repetitions', request.repetitions))
    + field('Concurrent attempts', integer('parallelism', request.parallelism))
    + field('Repairs per attempt', integer('repairs', request.repairs, 0))
    + field('Minutes per attempt', integer('timeoutMinutes', request.timeoutMinutes))
    + field('Cost cap per attempt (USD)', `<input name="maxCostUsd" type="number" min="0.01" step="0.01" value="${request.maxCostUsd}" required>`)
    + '</div><details><summary>Pause, run name and accounts</summary><div class="setup-fields">'
    + field('Pause', `<select name="pauseAfterDepth">${option('', 'No pause', request.pauseAfterDepth === null)}`
      + (w.workSelection === 'progressive' ? w.levels.filter(n => n < request.level).map(n => option(n, `After L${n}`, request.pauseAfterDepth === n)).join('') : '') + '</select>')
    + field('Run name', `<input name="key" value="${esc(request.key)}" pattern="[a-z0-9][a-z0-9.-]{2,119}" required>`)
    + [...new Set(w.agents.map(a => a.adapter))].map(adapter => field(esc(adapter), `<select name="credential-${esc(adapter)}">${option('', 'Automatic', !request.credentials.adapters?.[adapter])}`
      + catalog.profiles.filter(p => p.provider === w.agents.find(a => a.adapter === adapter)?.provider).map(p => option(p.id, `${p.id} (${p.mode})`, p.id === request.credentials.adapters?.[adapter])).join('') + '</select>')).join('')
    + '</div></details><div class="setup-actions">'
    + '<button class="btn primary" type="submit">Review run</button></div>' + alert + '</form></div>';
}
