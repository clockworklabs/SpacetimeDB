// One graph for the campaign: columns are depth, bands are questlines, edges
// are the catalog's own dependencies. Every stack builds the same catalog, so a
// node carries one dot per stack in fixed order. The renderer takes one
// node-status snapshot per stack, which is what the replay feeds it per step.

import type { CampaignProgression } from '../dashboard-views.js';
import { esc, stackLabel } from './format.js';

export interface GraphStack {
  stack: string;
  statuses: readonly string[];
}

const DOT: Record<string, string> = { passed: 'p', active: 'a', working: 'a', failed: 'f',
  blocked: 'b', locked: 'o' };
const COLUMN = 260;
const NODE_W = 200;
const ROW = 30;

interface Placed {
  x: number;
  y: number;
  index: number;
}

export function graph(view: CampaignProgression, stacks: readonly GraphStack[]): string {
  const depths = view.depths;
  const width = 150 + Math.max(1, depths.length) * COLUMN - 40;
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
      placed.set(node.id, { x: 150 + Math.max(0, depths.indexOf(node.depth)) * COLUMN,
        y: top + 8 + row * ROW, index: view.nodes.indexOf(node) });
    }
    const height = rows * ROW + 16;
    bands.push(`<text class="band" x="8" y="${top + 24}">${esc(questline.title)}</text>`);
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
      return [`<path class="e${cut ? ' cut' : ''}" d="M${source.x + NODE_W} ${source.y + 12}`
        + ` C ${source.x + NODE_W + 40} ${source.y + 12}, ${target.x - 40} ${target.y + 12},`
        + ` ${target.x} ${target.y + 12}"/>`];
    });
  });
  const nodes = view.nodes.map(node => {
    const at = placed.get(node.id);
    if (!at) return '';
    const dots = stacks.map((entry, column) =>
      `<circle class="d ${DOT[entry.statuses[at.index] ?? 'locked'] ?? 'o'}" `
      + `cx="${at.x + NODE_W - 40 + column * 14}" cy="${at.y + 12}" r="4"/>`).join('');
    const hover = stacks.map(entry =>
      `${stackLabel(entry.stack)} ${entry.statuses[at.index] ?? 'locked'}`).join(' · ');
    return `<g class="n"><title>${esc(`${node.title} · ${hover}`)}</title>`
      + `<rect x="${at.x}" y="${at.y}" width="${NODE_W}" height="24" rx="4"/>`
      + `<text x="${at.x + 9}" y="${at.y + 16}">${esc(node.title.length > 22
        ? `${node.title.slice(0, 21)}…` : node.title)}</text>${dots}</g>`;
  });
  const columns = depths.map((depth, index) =>
    `<text class="col" x="${150 + index * COLUMN}" y="14">depth ${depth}</text>`).join('');
  return `<svg class="dag" viewBox="0 0 ${width} ${top + 10}" role="img">`
    + `${bands.join('')}${columns}${edges.join('')}${nodes.join('')}</svg>`;
}
