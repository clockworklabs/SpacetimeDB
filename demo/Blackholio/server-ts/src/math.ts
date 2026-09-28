import { t, type Infer } from 'spacetimedb/server';

export const DbVector2 = t.object('DbVector2', {
  x: t.f32(),
  y: t.f32(),
});

export type DbVector2 = Infer<typeof DbVector2>;

export function vec(x: number, y: number): DbVector2 {
  return { x, y };
}

export function add(a: DbVector2, b: DbVector2): DbVector2 {
  return { x: a.x + b.x, y: a.y + b.y };
}

export function sub(a: DbVector2, b: DbVector2): DbVector2 {
  return { x: a.x - b.x, y: a.y - b.y };
}

export function mul(a: DbVector2, scalar: number): DbVector2 {
  return { x: a.x * scalar, y: a.y * scalar };
}

export function div(a: DbVector2, scalar: number): DbVector2 {
  return scalar === 0 ? { x: 0, y: 0 } : { x: a.x / scalar, y: a.y / scalar };
}

export function sqrMagnitude(a: DbVector2): number {
  return a.x * a.x + a.y * a.y;
}

export function magnitude(a: DbVector2): number {
  return Math.sqrt(sqrMagnitude(a));
}

export function normalized(a: DbVector2): DbVector2 {
  return div(a, magnitude(a));
}
