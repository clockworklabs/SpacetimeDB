// Pathfinding over an abstract graph. Caller injects `cost` and `neighbors`
// so this module knows nothing about STDB or grid kind. cost <= 0 = blocked.

import type { Coord } from './coords.ts';
import { coordKey } from './coords.ts';

export type PathResult =
  | { found: true; cells: Coord[]; cost: number; expanded: number }
  | { found: false; expanded: number };

export interface PathfindOpts {
  start: Coord;
  goal: Coord;
  cost: (c: Coord) => number;
  neighbors: (c: Coord) => Coord[];
  heuristic?: (c: Coord, goal: Coord) => number;
  maxExpansions?: number;
}

const DEFAULT_MAX_EXPANSIONS = 100_000;

// Binary min-heap. Each item carries its priority `key`, avoiding an
// external decrease-key. Duplicate pushes allow stale pops to be skipped.
interface HeapEntry {
  key: number;
  node: Coord;
  gScore: number;
}

class MinHeap {
  private items: HeapEntry[] = [];

  get size(): number {
    return this.items.length;
  }

  push(e: HeapEntry): void {
    this.items.push(e);
    this.siftUp(this.items.length - 1);
  }

  pop(): HeapEntry | undefined {
    const n = this.items.length;
    if (n === 0) return undefined;
    const top = this.items[0];
    const last = this.items.pop()!;
    if (n > 1) {
      this.items[0] = last;
      this.siftDown(0);
    }
    return top;
  }

  private siftUp(i: number): void {
    const a = this.items;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (a[p].key <= a[i].key) break;
      [a[p], a[i]] = [a[i], a[p]];
      i = p;
    }
  }

  private siftDown(i: number): void {
    const a = this.items;
    const n = a.length;
    while (true) {
      const l = i * 2 + 1;
      const r = l + 1;
      let s = i;
      if (l < n && a[l].key < a[s].key) s = l;
      if (r < n && a[r].key < a[s].key) s = r;
      if (s === i) break;
      [a[s], a[i]] = [a[i], a[s]];
      i = s;
    }
  }
}

export function findPathAstar(opts: PathfindOpts): PathResult {
  const { start, goal, cost, neighbors } = opts;
  const heuristic = opts.heuristic ?? (() => 0);
  const cap = opts.maxExpansions ?? DEFAULT_MAX_EXPANSIONS;

  const startKey = coordKey(start);
  const goalKey = coordKey(goal);

  if (startKey === goalKey) {
    return { found: true, cells: [start], cost: 0, expanded: 0 };
  }

  const gScore = new Map<string, number>();
  const parent = new Map<string, Coord>();
  const open = new MinHeap();

  gScore.set(startKey, 0);
  open.push({ key: heuristic(start, goal), node: start, gScore: 0 });

  let expanded = 0;
  while (open.size > 0) {
    if (expanded >= cap) break;
    const cur = open.pop()!;
    const curKey = coordKey(cur.node);

    // Skip stale heap entries (a better path landed after this one was pushed).
    if (cur.gScore > (gScore.get(curKey) ?? Infinity)) continue;
    expanded++;

    if (curKey === goalKey) {
      const path: Coord[] = [];
      let n: Coord | undefined = cur.node;
      while (n !== undefined) {
        path.unshift(n);
        n = parent.get(coordKey(n));
      }
      return { found: true, cells: path, cost: cur.gScore, expanded };
    }

    for (const nb of neighbors(cur.node)) {
      const stepCost = cost(nb);
      if (stepCost <= 0) continue;
      const tentativeG = cur.gScore + stepCost;
      const nbKey = coordKey(nb);
      if (tentativeG < (gScore.get(nbKey) ?? Infinity)) {
        gScore.set(nbKey, tentativeG);
        parent.set(nbKey, cur.node);
        open.push({
          key: tentativeG + heuristic(nb, goal),
          node: nb,
          gScore: tentativeG,
        });
      }
    }
  }

  return { found: false, expanded };
}

export interface DijkstraOpts {
  start: Coord;
  cost: (c: Coord) => number;
  neighbors: (c: Coord) => Coord[];
  maxCost: number;
  maxExpansions?: number;
}

export interface DijkstraNode {
  cell: Coord;
  cost: number;
  from: Coord | null;
}

// Returns all cells reachable from `start` within `maxCost`, keyed by coordKey.
export function dijkstra(opts: DijkstraOpts): Map<string, DijkstraNode> {
  const { start, cost, neighbors, maxCost } = opts;
  const cap = opts.maxExpansions ?? DEFAULT_MAX_EXPANSIONS;

  const result = new Map<string, DijkstraNode>();
  const open = new MinHeap();

  result.set(coordKey(start), { cell: start, cost: 0, from: null });
  open.push({ key: 0, node: start, gScore: 0 });

  let expanded = 0;
  while (open.size > 0) {
    if (expanded >= cap) break;
    const cur = open.pop()!;
    const curKey = coordKey(cur.node);
    const known = result.get(curKey);
    if (!known || cur.gScore > known.cost) continue;
    if (cur.gScore > maxCost) break;
    expanded++;

    for (const nb of neighbors(cur.node)) {
      const stepCost = cost(nb);
      if (stepCost <= 0) continue;
      const tentative = cur.gScore + stepCost;
      if (tentative > maxCost) continue;
      const nbKey = coordKey(nb);
      const existing = result.get(nbKey);
      if (!existing || tentative < existing.cost) {
        result.set(nbKey, { cell: nb, cost: tentative, from: cur.node });
        open.push({ key: tentative, node: nb, gScore: tentative });
      }
    }
  }

  return result;
}
