// The climb: one point per completed grade, unaided grades ringed, the current
// grade filled. Small in a lane or a sheet cell, large on the attempt page.

import type { ClimbPoint } from '../dashboard-views.js';
import { esc } from './format.js';

interface Plot {
  x: number;
  y: number;
  point: ClimbPoint;
}

function plot(series: readonly ClimbPoint[], left: number, right: number,
  top: number, bottom: number): Plot[] {
  const span = Math.max(1, series.length - 1);
  return series.map((point, index) => ({
    x: series.length === 1 ? (left + right) / 2 : left + (right - left) * index / span,
    y: bottom - (bottom - top) * (point.max ? point.score / point.max : 0),
    point,
  }));
}

function stepPath(plots: readonly Plot[]): string {
  const head = plots[0];
  if (!head) return '';
  return plots.slice(1).reduce((path, item, index) =>
    `${path} L${item.x} ${plots[index]!.y} L${item.x} ${item.y}`, `M${head.x} ${head.y}`);
}

export function climb(series: readonly ClimbPoint[], { warn = false, height = 36 }: {
  warn?: boolean;
  height?: number;
} = {}): string {
  if (!series.length) return '';
  const top = 4;
  const bottom = height - 4;
  const plots = plot(series, 8, 292, top, bottom);
  const line = stepPath(plots);
  const first = plots[0]!;
  const last = plots.at(-1)!;
  const tone = warn ? ' warn' : '';
  const rings = plots.filter(item => item.point.unaided || item === first)
    .map(item => `<circle class="first" cx="${item.x}" cy="${item.y}" r="3.2"/>`).join('');
  return `<svg class="climb" viewBox="0 0 300 ${height}" preserveAspectRatio="none">`
    + `<line class="grid" x1="0" y1="${top}" x2="300" y2="${top}"/>`
    + `<line class="grid" x1="0" y1="${bottom}" x2="300" y2="${bottom}"/>`
    + `<path class="area${tone}" d="M${first.x} ${bottom} ${line.slice(1)} L${last.x} ${bottom} Z"/>`
    + `<path class="line${tone}" d="${line}"/>${rings}`
    + `<circle class="now${tone}" cx="${last.x}" cy="${last.y}" r="3.2"/></svg>`;
}

// Full size: the same points with a band per depth or level, and a number at
// the first, the best and the current grade.
export function bigClimb(series: readonly ClimbPoint[], stage: (level: number) => string): string {
  if (!series.length) return '';
  const top = 10;
  const bottom = 130;
  const plots = plot(series, 100, 1010, top, bottom);
  const bands: string[] = [];
  let start = 0;
  plots.forEach((item, index) => {
    const next = plots[index + 1];
    if (next && next.point.level === item.point.level) return;
    const level = item.point.level;
    if (level !== null) {
      const from = Math.max(60, plots[start]!.x - 40);
      const width = Math.min(1050, item.x + 40) - from;
      bands.push(`<rect class="band" x="${from}" y="${top}" width="${width}" height="${bottom - top}"`
        + `${(bands.length % 2) ? ' opacity=".6"' : ''}/>`
        + `<text x="${from + width / 2}" y="152" text-anchor="middle">${esc(stage(level))}</text>`);
    }
    start = index + 1;
  });
  const line = stepPath(plots);
  const first = plots[0]!;
  const last = plots.at(-1)!;
  const best = plots.reduce((top1, item) => item.y < top1.y ? item : top1, first);
  const label = (item: Plot, tone: string): string =>
    `<text x="${item.x}" y="${item.y - 8 < top + 4 ? item.y + 16 : item.y - 8}" `
    + `text-anchor="middle" fill="${tone}">`
    + `${Math.round(item.point.max ? 100 * item.point.score / item.point.max : 0)}</text>`;
  return `<svg class="bigclimb" viewBox="0 0 1060 170" preserveAspectRatio="none">${bands.join('')}`
    + [0, 50, 100].map(value => {
      const y = bottom - (bottom - top) * value / 100;
      return `<line class="g" x1="60" y1="${y}" x2="1050" y2="${y}"/>`
        + `<text x="50" y="${y + 4}" text-anchor="end">${value}</text>`;
    }).join('')
    + `<path class="a" d="M${first.x} ${bottom} ${line.slice(1)} L${last.x} ${bottom} Z"/>`
    + `<path class="l" d="${line}"/>`
    + plots.map(item => `<circle class="ev${item.point.unaided || item === first ? ' first' : ''}`
      + `${item === last ? ' now' : ''}" cx="${item.x}" cy="${item.y}" r="${
        item.point.unaided || item === last ? 4.5 : 3.5}"/>`).join('')
    + label(first, '#b6c0cf') + (best === first || best === last ? '' : label(best, '#b6c0cf'))
    + (last === first ? '' : label(last, '#e6e9f0')) + '</svg>';
}
