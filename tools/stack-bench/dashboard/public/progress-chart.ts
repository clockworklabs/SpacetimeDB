import type { CampaignProgression, CampaignSheet } from '../dashboard-views.js';
import { duration, esc, stackLabel } from './format.js';

export function progressChart(sheet: CampaignSheet, progression: CampaignProgression | null): string {
  const tracks = (progression?.stacks ?? []).flatMap(track => {
    const attempt = sheet.stacks.find(stack => stack.stack === track.stack)?.attempts
      .find(candidate => candidate.id === track.attemptId);
    const start = Date.parse(attempt?.executionStartedAt ?? '');
    if (!attempt || !Number.isFinite(start)) return [];
    const observations = track.steps.flatMap(step => {
      const elapsed = (Date.parse(step.completedAt ?? '') - start) / 1000;
      return Number.isFinite(elapsed) && elapsed >= 0 && step.completion != null
        ? [{ elapsed, completion: step.completion * 100 }] : [];
    }).sort((a, b) => a.elapsed - b.elapsed);
    const points = [{ elapsed: 0, completion: 0 }, ...observations];
    return observations.length ? [{ stack: track.stack, attempt, points }] : [];
  });
  const heading = '<h3 title="Checks passed out of all selected checks at each saved grade. Zero marks run start before any checks pass. Each line is one repetition; elapsed time starts at that run. Excluded runs are labelled. Lines can fall after regressions.">Completion over time</h3>';
  if (!tracks.length) return heading + '<p class="chart-empty">Awaiting first timed grade.</p>';
  const maximum = Math.max(60, ...tracks.flatMap(track => track.points.map(point => point.elapsed)));
  const x = (seconds: number) => 48 + 900 * seconds / maximum;
  const y = (completion: number) => 190 - 1.6 * completion;
  const color = (stack: string) => `hsl(${(sheet.stacks.findIndex(entry => entry.stack === stack) * 137.508 + 150) % 360},65%,65%)`;
  const grid = [0, 25, 50, 75, 100].map(value =>
    `<line x1="48" x2="948" y1="${y(value)}" y2="${y(value)}" class="progress-grid"/><text x="38" y="${y(value) + 4}" text-anchor="end">${value}%</text>`).join('');
  const ticks = [0, 0.25, 0.5, 0.75, 1].map(part =>
    `<text x="${x(part * maximum)}" y="214" text-anchor="middle">${esc(part ? duration(part * maximum) : '0')}</text>`).join('');
  const lines = tracks.map(({ stack, attempt, points }) => {
    // A grade is an observation, not continuous progress. Do not extend beyond it.
    let path = '';
    const marks = points.map((point, index) => {
      path += index ? ` H${x(point.elapsed)} V${y(point.completion)}` : `M${x(point.elapsed)} ${y(point.completion)}`;
      return `<circle cx="${x(point.elapsed)}" cy="${y(point.completion)}" r="3" fill="${color(stack)}"><title>${esc(stackLabel(stack))} · Rep ${attempt.repetition}: ${point.completion.toFixed(1)}% at ${esc(duration(point.elapsed))}${index === 0 ? ' � Run start; no checks graded' : ''}${attempt.excluded ? ' · Excluded' : ''}</title></circle>`;
    }).join('');
    return `<g><path d="${path}" fill="none" stroke="${color(stack)}" stroke-width="2"${attempt.repetition > 1 ? ' stroke-dasharray="6 4"' : ''}/>${marks}</g>`;
  }).join('');
  const legend = tracks.map(({ stack, attempt, points }) => `<a href="/c/${encodeURIComponent(sheet.key)}/a/${encodeURIComponent(attempt.id)}"><svg width="16" height="12" aria-hidden="true"><path d="M0 6 H16" stroke="${color(stack)}" stroke-width="2"${attempt.repetition > 1 ? ' stroke-dasharray="4 2"' : ''}/></svg> ${esc(stackLabel(stack))} · Rep ${attempt.repetition} · ${points.at(-1)!.completion.toFixed(0)}%${attempt.excluded ? ' · Excluded' : ''}</a>`).join('');
  return heading + '<div class="chart-scroll" role="region" aria-label="Completion over elapsed time" tabindex="0">'
    + `<svg class="progress-chart" viewBox="0 0 980 242" role="img" aria-label="Check completion by elapsed run time"><title>Recorded check completion. Hover a point for its grade time and value.</title>${grid}${ticks}${lines}<text x="498" y="237" text-anchor="middle">Elapsed run time</text></svg></div>`
    + `<div class="progress-legend">${legend}</div>`;
}
