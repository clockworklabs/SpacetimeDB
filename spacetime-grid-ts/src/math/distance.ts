// Grid distance functions. Choose based on connectivity:
//   square 4-connected: manhattan
//   square 8-connected: chebyshev
//   hex (any orientation): hexDistance

import type { Coord, GridKind, Connectivity } from './coords.ts';

export function manhattan(a: Coord, b: Coord): number {
  return Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
}

export function chebyshev(a: Coord, b: Coord): number {
  return Math.max(Math.abs(a.x - b.x), Math.abs(a.y - b.y));
}

// Hex axial distance. (q,r) stored as (x,y).
export function hexDistance(a: Coord, b: Coord): number {
  return (
    (Math.abs(a.x - b.x) +
      Math.abs(a.x + a.y - b.x - b.y) +
      Math.abs(a.y - b.y)) /
    2
  );
}

export function distance(
  kind: GridKind,
  a: Coord,
  b: Coord,
  connectivity: Connectivity = 4
): number {
  if (kind === 'hex') return hexDistance(a, b);
  return connectivity === 8 ? chebyshev(a, b) : manhattan(a, b);
}
