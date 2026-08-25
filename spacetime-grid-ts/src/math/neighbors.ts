// Neighbor offsets per grid kind. Hex axial is orientation-agnostic.

import type { Coord, Connectivity, GridKind } from './coords.ts';

const SQUARE_4: ReadonlyArray<readonly [number, number]> = [
  [0, -1],
  [1, 0],
  [0, 1],
  [-1, 0],
];

const SQUARE_8: ReadonlyArray<readonly [number, number]> = [
  [0, -1],
  [1, -1],
  [1, 0],
  [1, 1],
  [0, 1],
  [-1, 1],
  [-1, 0],
  [-1, -1],
];

const HEX_AXIAL: ReadonlyArray<readonly [number, number]> = [
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
  [-1, 1],
  [0, 1],
];

export function neighbors(
  kind: GridKind,
  c: Coord,
  connectivity: Connectivity = 4
): Coord[] {
  const offsets =
    kind === 'hex' ? HEX_AXIAL : connectivity === 8 ? SQUARE_8 : SQUARE_4;
  const out: Coord[] = [];
  for (const [dx, dy] of offsets) {
    out.push({ x: c.x + dx, y: c.y + dy });
  }
  return out;
}
