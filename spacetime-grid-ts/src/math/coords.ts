// Pure coordinate types. No STDB. Square (x,y) and hex axial (q=x, r=y).

export type Coord = { x: number; y: number };
export type GridKind = 'square' | 'hex';
export type HexOrientation = 'flat' | 'pointy';
export type Connectivity = 4 | 8;

export function coordKey(c: Coord): string {
  return `${c.x},${c.y}`;
}

export function parseCoordKey(key: string): Coord {
  const i = key.indexOf(',');
  return { x: Number(key.slice(0, i)), y: Number(key.slice(i + 1)) };
}

export function coordsEqual(a: Coord, b: Coord): boolean {
  return a.x === b.x && a.y === b.y;
}
