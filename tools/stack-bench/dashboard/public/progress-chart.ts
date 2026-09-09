import type { CampaignProgression, CampaignSheet } from '../dashboard-views.js';
import { duration, esc, stackLabel } from './format.js';

export function progressChart(sheet: CampaignSheet, progression: CampaignProgression | null,
  metric: 'completion' | 'cost' | 'distribution' = 'completion', view = 'grid', hidden: ReadonlySet<string> = new Set(), unit: 'checks' | 'features' = 'features'): string {
  const tracks = metric === 'distribution' ? sheet.stacks.flatMap(stack => stack.attempts.flatMap(attempt => {
    const rate = unit === 'features' ? attempt.featureCompletion?.rate : attempt.completion?.rate;
    return attempt.status === 'completed' && rate != null && Number.isFinite(rate)
      ? [{ stack: stack.stack, attempt, points: [{ elapsed: 0, value: rate * 100, upper: false }] }] : [];
  })) : (progression?.stacks ?? []).flatMap(track => {
    const attempt = sheet.stacks.find(stack => stack.stack === track.stack)?.attempts
      .find(candidate => candidate.id === track.attemptId);
    const start = Date.parse(attempt?.executionStartedAt ?? '');
    if (!attempt || !Number.isFinite(start)) return [];
    const observations = (metric === 'cost' ? (track.costs ?? []).map(point => ({
      completedAt: point.completedAt, value: point.cost.costUsd, upper: point.cost.status === 'upper-bound',
    })) : track.steps.map(step => ({ completedAt: step.completedAt,
      value: (unit === 'features' ? step.featureCompletion : step.completion) == null ? null
        : (unit === 'features' ? step.featureCompletion! : step.completion!) * 100, upper: false }))).flatMap(step => {
      const elapsed = (Date.parse(step.completedAt ?? '') - start) / 1000;
      return Number.isFinite(elapsed) && elapsed >= 0 && step.value != null
        && Number.isFinite(step.value) && step.value >= 0
        ? [{ elapsed, value: step.value, upper: step.upper }] : [];
    }).sort((a, b) => a.elapsed - b.elapsed);
    const points = [{ elapsed: 0, value: 0, upper: false }, ...observations];
    return observations.length ? [{ stack: track.stack, attempt, points }] : [];
  });
  const unitDescription = unit === 'features'
    ? 'Features fully passed out of all selected features. A feature passes only when all its selected checks pass, including production guarantees.'
    : 'Accepted checks passed out of all selected checks.';
  const label = metric === 'distribution' ? 'Completion distribution' : metric === 'cost' ? 'Cost' : 'Completion';
  const description = metric === 'distribution'
    ? `One point per completed run, grouped by provider. ${unitDescription} Running runs are omitted. Excluded runs are labelled.`
    : metric === 'cost'
    ? 'Cumulative cost per run at saved grade checkpoints. Includes repairs and excluded runs. Subscription costs use the pinned API-equivalent price snapshot, not invoice charges. Unknown costs are not plotted; upper bounds are labelled. Time starts at the current execution. Lines connect recorded observations; intermediate values are not measured.'
    : `${unitDescription} Each point is a saved grade. Zero marks run start. Each line is one repetition; elapsed time starts at that run. Excluded runs are labelled. Lines can fall after regressions. Intermediate values are not measured.`;
  const heading = `<div class="section-heading progress-heading"><h3 title="${description}">${label}${metric === 'distribution' ? '' : ' over time'}</h3><div class="chart-options"><nav aria-label="Chart metric">`
    + (['completion', 'cost', 'distribution'] as const).map(option => `<a class="chip sm${metric === option ? ' on' : ''}"${metric === option ? ' aria-current="page"' : ''} href="?questlines=${encodeURIComponent(view)}&amp;chart=${option}&amp;unit=${unit}">${option === 'distribution' ? 'Distribution' : option === 'cost' ? 'Cost' : 'Completion'}</a>`).join('') + '</nav>'
    + '<nav aria-label="Completion unit">'
    + (['features', 'checks'] as const).map(option => {
      const classes = `chip sm${unit === option ? ' on' : ''}`;
      const text = option === 'features' ? 'Features' : 'Checks';
      return metric === 'cost'
        ? `<span class="${classes}" role="link" aria-disabled="true">${text}</span>`
        : `<a class="${classes}"${unit === option ? ' aria-current="page"' : ''} href="?questlines=${encodeURIComponent(view)}&amp;chart=${metric}&amp;unit=${option}">${text}</a>`;
    }).join('') + '</nav>'
    + '</div></div>';
  const valueLabel = (value: number, upper = false, decimals = 1) => metric === 'cost'
    ? `${upper ? '≤' : ''}$${value.toFixed(2)}` : `${value.toFixed(decimals)}%`;
  const brandColors: Record<string, string> = { spacetime: '#4cf490', mongodb: '#b45af2', postgres: '#336791' };
  const color = (stack: string) => brandColors[stack]
    ?? `hsl(${Array.from(stack).reduce((hash, char) => (hash * 31 + char.charCodeAt(0)) % 360, 0)},65%,65%)`;
  const marker = (repetition: number, x: number, y: number, title = '') => {
    const shape = (repetition - 1) % 3;
    return shape === 1 ? `<rect x="${x - 3}" y="${y - 3}" width="6" height="6">${title}</rect>`
      : shape === 2 ? `<path d="M${x} ${y - 4} l4 4 -4 4 -4 -4 Z">${title}</path>`
      : `<circle cx="${x}" cy="${y}" r="3">${title}</circle>`;
  };
  const controls = `<div class="progress-controls" aria-label="Chart visibility">` + sheet.stacks.map(stack => {
    const shown = stack.attempts.filter(attempt => !hidden.has(attempt.id)).length;
    return `<div class="progress-stack" role="group" aria-label="${esc(stackLabel(stack.stack))}">`
      + `<button type="button" class="chart-stack-toggle" data-chart-stack="${esc(stack.stack)}" aria-pressed="${shown === 0 ? 'false' : shown === stack.attempts.length ? 'true' : 'mixed'}" title="Show or hide all ${esc(stackLabel(stack.stack))} runs"><svg class="chart-swatch" width="16" height="12" aria-hidden="true"><path d="M0 6 H16" stroke="${color(stack.stack)}" stroke-width="3"/></svg>${esc(stackLabel(stack.stack))}</button>`
      + '<div class="chart-runs">' + stack.attempts.map(attempt => {
        const point = tracks.find(track => track.attempt.id === attempt.id)?.points.at(-1);
        const label = `Rep ${attempt.repetition} · ${point ? valueLabel(point.value, point.upper, 0) : 'Pending'}${attempt.excluded ? ' · Excluded' : ''}`;
        return `<button type="button" class="chart-run-toggle" data-chart-run="${esc(attempt.id)}" data-chart-series="${esc(attempt.id)}" aria-pressed="${!hidden.has(attempt.id)}" aria-label="${esc(stackLabel(stack.stack))} · ${esc(label)}" title="Show or hide ${esc(stackLabel(stack.stack))} repetition ${attempt.repetition}"><svg width="12" height="12" fill="${color(stack.stack)}" aria-hidden="true">${marker(attempt.repetition, 6, 6)}</svg>${esc(label)}</button>`;
      }).join('') + '</div></div>';
  }).join('') + '</div>';
  const visible = tracks.filter(track => !hidden.has(track.attempt.id));
  if (!visible.length) return `<section class="progress-panel">${heading}${controls}<p class="chart-empty">${tracks.length > 0 && tracks.every(track => hidden.has(track.attempt.id)) ? 'Select a run to show its progress.' : metric === 'distribution' ? 'Awaiting first completed run.' : metric === 'cost' ? 'Awaiting first timed cost receipt.' : 'Awaiting first timed grade.'}</p></section>`;
  if (metric === 'distribution') {
    const axisLeft = 150;
    const position = (value: number) => axisLeft + (910 - axisLeft) * value / 100;
    const rowHeight = Math.max(70, ...sheet.stacks.map(stack => stack.attempts.length * 20 + 24));
    const bottom = sheet.stacks.length * rowHeight + 24;
    const ticks = [0, 25, 50, 75, 100].map(value =>
      `<line class="progress-grid" x1="${position(value)}" x2="${position(value)}" y1="12" y2="${bottom}"/>`
      + `<text x="${position(value)}" y="${bottom + 22}" text-anchor="middle">${value}%</text>`).join('');
    const rows = sheet.stacks.map((stack, index) => {
      const center = 24 + rowHeight * (index + 0.5);
      const points = visible.filter(track => track.stack === stack.stack);
      return `<text x="12" y="${center + 4}">${esc(stackLabel(stack.stack))}</text>`
        + points.map(track => {
          const value = track.points[0]!.value;
          const at = center + (stack.attempts.findIndex(a => a.id === track.attempt.id) - (stack.attempts.length - 1) / 2) * 20;
          return `<g data-chart-series="${esc(track.attempt.id)}" fill="${color(stack.stack)}">`
            + marker(track.attempt.repetition, position(value), at, `<title>${esc(stackLabel(stack.stack))} / Rep ${track.attempt.repetition}: ${valueLabel(value)}${track.attempt.excluded ? ' / Excluded' : ''}</title>`)
            + `<text x="${position(value) + 8}" y="${at + 4}">${valueLabel(value, false, 0)}</text></g>`;
        }).join('');
    }).join('');
    return `<section class="progress-panel">${heading}${controls}<div class="chart-scroll" role="region" aria-label="Completion distribution by provider" tabindex="0">`
      + `<svg class="progress-chart" viewBox="0 0 980 ${bottom + 45}" role="img" aria-label="Completion distribution by provider"><title>${description}</title>${ticks}${rows}</svg></div></section>`;
  }
  const ceiling = metric === 'cost' ? Math.max(0.01, ...tracks.flatMap(track => track.points.map(point => point.value))) : 100;
  const maximum = Math.max(60, ...tracks.flatMap(track => track.points.map(point => point.elapsed)));
  const left = metric === 'cost' ? 80 : 48;
  const x = (seconds: number) => left + (948 - left) * seconds / maximum;
  const y = (value: number) => 190 - 160 * value / ceiling;
  const grid = [0, 0.25, 0.5, 0.75, 1].map(part => part * ceiling).map(value =>
    `<line x1="${left}" x2="948" y1="${y(value)}" y2="${y(value)}" class="progress-grid"/><text x="${left - 10}" y="${y(value) + 4}" text-anchor="end">${valueLabel(value, false, 0)}</text>`).join('');
  const ticks = [0, 0.25, 0.5, 0.75, 1].map(part =>
    `<text x="${x(part * maximum)}" y="214" text-anchor="middle">${esc(part ? duration(part * maximum) : '0')}</text>`).join('');
  const lines = visible.map(({ stack, attempt, points }) => {
    // Saved observations are not continuous measurements. Stop at the last receipt.
    let path = '';
    const marks = points.map((point, index) => {
      path += index ? ` L${x(point.elapsed)} ${y(point.value)}` : `M${x(point.elapsed)} ${y(point.value)}`;
      return marker(attempt.repetition, x(point.elapsed), y(point.value), `<title>${esc(stackLabel(stack))} · Rep ${attempt.repetition}: ${valueLabel(point.value, point.upper)} at ${esc(duration(point.elapsed))}${index === 0 ? (metric === 'cost' ? ' · Run start; no recorded cost' : ' · Run start; no checks graded') : ''}${attempt.excluded ? ' · Excluded' : ''}</title>`);
    }).join('');
    return `<g class="progress-series" data-chart-series="${esc(attempt.id)}" fill="${color(stack)}"><path class="progress-line" d="${path}" fill="none" stroke="${color(stack)}" stroke-width="2"/>${marks}</g>`;
  }).join('');
  return `<section class="progress-panel">${heading}${controls}<div class="chart-scroll" role="region" aria-label="${label} over elapsed time" tabindex="0">`
    + `<svg class="progress-chart" viewBox="0 0 980 242" role="img" aria-label="${label} by elapsed run time"><title>${description} Hover a point for its time and value.</title>${grid}${ticks}${lines}<text x="498" y="237" text-anchor="middle">Elapsed run time</text></svg></div>`
    + '</section>';
}
