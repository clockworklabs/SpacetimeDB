import type { Vec2 } from './math';

export function pointerDirection(pointer: Vec2, viewport: Vec2): Vec2 {
  const center = { x: viewport.x / 2, y: viewport.y / 2 };
  const scale = viewport.y / 3;
  return {
    x: (pointer.x - center.x) / scale,
    y: (pointer.y - center.y) / scale,
  };
}
