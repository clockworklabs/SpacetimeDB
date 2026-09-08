import type { CampaignProgression, CampaignSheet } from '../dashboard-views.js';
import { duration, esc, stackLabel } from './format.js';

export function progressChart(sheet: CampaignSheet, progression: CampaignProgression | null,
  metric: 'completion' | 'cost' = 'completion', view = 'grid'): string {
  const tracks = (progression?.stacks ?? []).flatMap(track => {
    const attempt = sheet.stacks.find(stack => stack.stack === track.stack)?.attempts
      .find(candidate => candidate.id === track.attemptId);
    const start = Date.parse(attempt?.executionStartedAt ?? '');
    if (!attempt || !Number.isFinite(start)) return [];
    const observations = (metric === 'cost' ? (track.costs ?? []).map(point => ({
      completedAt: point.completedAt, value: point.cost.costUsd, upper: point.cost.status === 'upper-bound',
    })) : track.steps.map(step => ({ completedAt: step.completedAt,
      value: step.completion == null ? null : step.completion * 100, upper: false }))).flatMap(step => {
      const elapsed = (Date.parse(step.completedAt ?? '') - start) / 1000;
      return Number.isFinite(elapsed) && elapsed >= 0 && step.value != null
        && Number.isFinite(step.value) && step.value >= 0
        ? [{ elapsed, value: step.value, upper: step.upper }] : [];
    }).sort((a, b) => a.elapsed - b.elapsed);
    const points = [{ elapsed: 0, value: 0, upper: false }, ...observations];
    return observations.length ? [{ stack: track.stack, attempt, points }] : [];
  });
  const label = metric === 'cost' ? 'Cost' : 'Completion';
  const description = metric === 'cost'
    ? 'Cumulative cost per run at saved grade checkpoints. Includes repairs and excluded runs. Subscription costs use the pinned API-equivalent price snapshot, not invoice charges. Unknown costs are not plotted; upper bounds are labelled. Time starts at the current execution.'
    : 'Checks passed out of all selected checks at each saved grade. Zero marks run start before any checks pass. Each line is one repetition; elapsed time starts at that run. Excluded runs are labelled. Lines can fall after regressions.';
  const heading = `<div class="section-heading"><h3 title="${description}">${label} over time</h3><nav aria-label="Chart metric">`
    + (['completion', 'cost'] as const).map(option => `<a class="chip sm${metric === option ? ' on' : ''}"${metric === option ? ' aria-current="page"' : ''} href="?questlines=${encodeURIComponent(view)}&amp;chart=${option}">${option === 'cost' ? 'Cost' : 'Completion'}</a>`).join('') + '</nav></div>';
  if (!tracks.length) return heading + `<p class="chart-empty">${metric === 'cost' ? 'Awaiting first timed cost receipt.' : 'Awaiting first timed grade.'}</p>`;
  const ceiling = metric === 'cost' ? Math.max(0.01, ...tracks.flatMap(track => track.points.map(point => point.value))) : 100;
  const valueLabel = (value: number, upper = false, decimals = 1) => metric === 'cost'
    ? `${upper ? '≤' : ''}$${value.toFixed(2)}` : `${value.toFixed(decimals)}%`;
  const maximum = Math.max(60, ...tracks.flatMap(track => track.points.map(point => point.elapsed)));
  const left = metric === 'cost' ? 80 : 48;
  const x = (seconds: number) => left + (948 - left) * seconds / maximum;
  const y = (value: number) => 190 - 160 * value / ceiling;
  const color = (stack: string) => `hsl(${(sheet.stacks.findIndex(entry => entry.stack === stack) * 137.508 + 150) % 360},65%,65%)`;
  const grid = [0, 0.25, 0.5, 0.75, 1].map(part => part * ceiling).map(value =>
    `<line x1="${left}" x2="948" y1="${y(value)}" y2="${y(value)}" class="progress-grid"/><text x="${left - 10}" y="${y(value) + 4}" text-anchor="end">${valueLabel(value, false, 0)}</text>`).join('');
  const ticks = [0, 0.25, 0.5, 0.75, 1].map(part =>
    `<text x="${x(part * maximum)}" y="214" text-anchor="middle">${esc(part ? duration(part * maximum) : '0')}</text>`).join('');
  const lines = tracks.map(({ stack, attempt, points }) => {
    // Saved observations are not continuous measurements. Stop at the last receipt.
    let path = '';
    const marks = points.map((point, index) => {
      path += index ? ` H${x(point.elapsed)} V${y(point.value)}` : `M${x(point.elapsed)} ${y(point.value)}`;
      return `<circle cx="${x(point.elapsed)}" cy="${y(point.value)}" r="3" fill="${color(stack)}"><title>${esc(stackLabel(stack))} · Rep ${attempt.repetition}: ${valueLabel(point.value, point.upper)} at ${esc(duration(point.elapsed))}${index === 0 ? (metric === 'cost' ? ' · Run start; no recorded cost' : ' · Run start; no checks graded') : ''}${attempt.excluded ? ' · Excluded' : ''}</title></circle>`;
    }).join('');
    return `<g><path d="${path}" fill="none" stroke="${color(stack)}" stroke-width="2"${attempt.repetition > 1 ? ' stroke-dasharray="6 4"' : ''}/>${marks}</g>`;
  }).join('');
  const legend = tracks.map(({ stack, attempt, points }) => `<a href="/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}"><svg width="16" height="12" aria-hidden="true"><path d="M0 6 H16" stroke="${color(stack)}" stroke-width="2"${attempt.repetition > 1 ? ' stroke-dasharray="4 2"' : ''}/></svg> ${esc(stackLabel(stack))} · Rep ${attempt.repetition} · ${valueLabel(points.at(-1)!.value, points.at(-1)!.upper, 0)}${attempt.excluded ? ' · Excluded' : ''}</a>`).join('');
  return heading + `<div class="chart-scroll" role="region" aria-label="${label} over elapsed time" tabindex="0">`
    + `<svg class="progress-chart" viewBox="0 0 980 242" role="img" aria-label="${label} by elapsed run time"><title>${description} Hover a point for its time and value.</title>${grid}${ticks}${lines}<text x="498" y="237" text-anchor="middle">Elapsed run time</text></svg></div>`
    + `<div class="progress-legend">${legend}</div>`;
}
