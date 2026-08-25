export const HEX_SIZE = 26;
export const HEX_H = Math.sqrt(3) * HEX_SIZE;
export const COL_STEP = 1.5 * HEX_SIZE;
export const GRID_RADIUS = 5;
export const HEX_MIN_X = -HEX_SIZE;
export const HEX_MAX_X = COL_STEP * 2 * GRID_RADIUS + HEX_SIZE;
export const HEX_MIN_Y = HEX_H * (GRID_RADIUS / 2) - HEX_H / 2;
export const HEX_MAX_Y =
  HEX_H * (2 * GRID_RADIUS + GRID_RADIUS / 2) + HEX_H / 2;
export const PAD = 12;
export const PAD_LEFT = -HEX_MIN_X + PAD;
export const PAD_TOP = -HEX_MIN_Y + PAD;

export function cellKey(x, y) {
  return `${x},${y}`;
}

// Keep this formula aligned with the Grid component's server distance rule.
export function axialHexDistance(ax, ay, bx, by) {
  return (
    (Math.abs(ax - bx) + Math.abs(ax + ay - bx - by) + Math.abs(ay - by)) / 2
  );
}

export function isInHexShape(q, r) {
  const center = GRID_RADIUS;
  return (
    (Math.abs(q - center) +
      Math.abs(r - center) +
      Math.abs(q + r - center * 2)) /
      2 <=
    GRID_RADIUS
  );
}

export function cellsWithinHexDistance(gridW, gridH, ox, oy, range) {
  const cells = new Set();
  for (let r = 0; r < gridH; r++) {
    for (let q = 0; q < gridW; q++) {
      if (isInHexShape(q, r) && axialHexDistance(ox, oy, q, r) <= range) {
        cells.add(cellKey(q, r));
      }
    }
  }
  return cells;
}

// Flat-top axial coordinates.
export function hexCenter(q, r) {
  return {
    cx: COL_STEP * q + PAD_LEFT,
    cy: HEX_H * (q / 2 + r) + PAD_TOP,
  };
}

export function hexCorners(cx, cy, size) {
  const points = [];
  for (let index = 0; index < 6; index++) {
    const angle = (Math.PI / 3) * index;
    points.push([cx + size * Math.cos(angle), cy + size * Math.sin(angle)]);
  }
  return points;
}

export function pixelToHex(px, py, gridW, gridH) {
  let closest = null;
  let closestDistance = Infinity;
  for (let y = 0; y < gridH; y++) {
    for (let x = 0; x < gridW; x++) {
      if (!isInHexShape(x, y)) continue;
      const { cx, cy } = hexCenter(x, y);
      const distance = (cx - px) ** 2 + (cy - py) ** 2;
      if (distance < closestDistance) {
        closestDistance = distance;
        closest = { x, y };
      }
    }
  }
  return closestDistance <= HEX_SIZE * HEX_SIZE ? closest : null;
}

export function samplePathPixels(path, progress) {
  if (path.length === 0) return { x: 0, y: 0 };
  if (path.length === 1 || progress <= 0) return { ...path[0] };
  if (progress >= 1) return { ...path[path.length - 1] };

  const segmentLengths = [];
  let totalLength = 0;
  for (let index = 1; index < path.length; index++) {
    const length = Math.hypot(
      path[index].x - path[index - 1].x,
      path[index].y - path[index - 1].y
    );
    segmentLengths.push(length);
    totalLength += length;
  }

  let targetLength = progress * totalLength;
  for (let index = 0; index < segmentLengths.length; index++) {
    const segmentLength = segmentLengths[index];
    if (targetLength <= segmentLength) {
      const ratio = segmentLength === 0 ? 0 : targetLength / segmentLength;
      return {
        x: path[index].x + (path[index + 1].x - path[index].x) * ratio,
        y: path[index].y + (path[index + 1].y - path[index].y) * ratio,
      };
    }
    targetLength -= segmentLength;
  }
  return { ...path[path.length - 1] };
}
