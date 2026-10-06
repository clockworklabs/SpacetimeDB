// One graph for the campaign: columns are depth, bands are questlines, edges
// are the catalog's own dependencies. Every stack builds the same catalog, so a
// node carries one dot per stack in fixed order. The renderer takes one
// node-status snapshot per stack, which is what the replay feeds it per step.

import type { CampaignProgression } from '../dashboard-views.js';
import { esc, stackLabel, statusWord } from './format.js';

export interface GraphStack {
  stack: string;
  statuses: readonly string[];
}

const DOT: Record<string, string> = { passed: 'p', active: 'a', working: 'a', failed: 'f',
  blocked: 'b', locked: 'o' };
const DOT_START = 180;
const DOT_SPACING = 14;
const ROW = 30;

interface Placed {
  x: number;
  y: number;
  index: number;
}

export function graph(view: CampaignProgression, stacks: readonly GraphStack[]): string {
  if (!view.nodes.length) return '<p class="chart-empty">No feature graph is available.</p>';
  const depths = view.depths;
  const nodeWidth = DOT_START + Math.max(1, stacks.length) * DOT_SPACING;
  const columnWidth = nodeWidth + 60;
  const width = 150 + Math.max(1, depths.length) * columnWidth - 40;
  const placed = new Map<string, Placed>();
  const bands: string[] = [];
  let top = 20;
  for (const questline of view.questlines) {
    const nodes = view.nodes.filter(node => node.questline === questline.id);
    if (!nodes.length) continue;
    const used = new Map<number, number>();
    let rows = 0;
    for (const node of nodes) {
      const row = used.get(node.depth) ?? 0;
      used.set(node.depth, row + 1);
      rows = Math.max(rows, row + 1);
      placed.set(node.id, { x: 150 + Math.max(0, depths.indexOf(node.depth)) * columnWidth,
        y: top + 8 + row * ROW, index: view.nodes.indexOf(node) });
    }
    const height = rows * ROW + 16;
    bands.push(`<text class="band" x="8" y="${top + 24}">${esc(questline.title.length > 19 ? `${questline.title.slice(0, 18)}…` : questline.title)}<title>${esc(questline.title)}</title></text>`);
    top += height;
    bands.push(`<line class="sep" x1="0" y1="${top}" x2="${width}" y2="${top}"/>`);
  }
  const failed = (index: number): boolean =>
    stacks.some(entry => entry.statuses[index] === 'failed');
  const blocked = (index: number): boolean =>
    stacks.some(entry => entry.statuses[index] === 'blocked');
  const edges = view.nodes.flatMap(node => {
    const target = placed.get(node.id);
    if (!target) return [];
    return node.dependencies.flatMap(id => {
      const source = placed.get(id);
      if (!source) return [];
      const cut = failed(source.index) || blocked(target.index);
      return [`<path class="e${cut ? ' cut' : ''}" d="M${source.x + nodeWidth} ${source.y + 12}`
        + ` C ${source.x + nodeWidth + 40} ${source.y + 12}, ${target.x - 40} ${target.y + 12},`
        + ` ${target.x} ${target.y + 12}"/>`];
    });
  });
  const nodes = view.nodes.map(node => {
    const at = placed.get(node.id);
    if (!at) return '';
    const dots = stacks.map((entry, column) =>
      `<circle class="d ${DOT[entry.statuses[at.index] ?? 'locked'] ?? 'o'}" `
      + `cx="${at.x + DOT_START + column * DOT_SPACING}" cy="${at.y + 12}" r="4"/>`
      + (entry.statuses[at.index] === 'passed'
        ? `<path class="passed-check" transform="translate(${at.x + DOT_START + column * DOT_SPACING} ${at.y + 12})" d="M-2 0 L-.5 1.5 L2 -1.5"/>` : '')).join('');
    const hover = stacks.map(entry =>
      `${stackLabel(entry.stack)} ${statusWord(entry.statuses[at.index] ?? 'locked')}`).join(' · ');
    return `<g class="n"><title>${esc(`${node.title} · ${hover}`)}</title>`
      + `<rect x="${at.x}" y="${at.y}" width="${nodeWidth}" height="24" rx="4"/>`
      + `<text x="${at.x + 9}" y="${at.y + 16}">${esc(node.title.length > 22
        ? `${node.title.slice(0, 21)}…` : node.title)}</text>${dots}</g>`;
  });
  const columns = depths.map((depth, index) =>
    `<text class="col" x="${150 + index * columnWidth}" y="14">depth ${depth}</text>`).join('');
  const order = stacks.map(entry => stackLabel(entry.stack)).join(', ');
  const description = view.nodes.map((node, index) => `${node.title}: ${stacks.map(entry =>
    `${stackLabel(entry.stack)} ${statusWord(entry.statuses[index] ?? 'locked')}`).join(', ')}`).join('. ');
  const key = [['p', 'Passed'], ['a', 'Active'], ['f', 'Failed'], ['b', 'Blocked'], ['o', 'Locked']]
    .map(([tone, label]) => `<span><i class="dot ${tone}" aria-hidden="true"></i>${label}</span>`).join('');
  return `<div class="graph-key">${stacks.length > 1 ? `<p>Feature dots, left to right: ${esc(order)}.</p>` : ''}${key}</div>`
    + `<div class="graph-scroll" role="region" aria-label="Feature dependency graph" tabindex="0">`
    + `<svg class="dag" width="${width}" viewBox="0 0 ${width} ${top + 10}" role="img" aria-label="Feature dependencies and stack status">`
    + `<title>Feature dependencies and stack status</title><desc>${esc(description)}</desc>`
    + `${bands.join('')}${columns}${edges.join('')}${nodes.join('')}</svg></div>`;
}
