import assert from 'node:assert/strict';
import {
  GRID_RADIUS,
  HEX_SIZE,
  axialHexDistance,
  cellKey,
  cellsWithinHexDistance,
  hexCenter,
  hexCorners,
  isInHexShape,
  pixelToHex,
  samplePathPixels,
} from '../public/hex-geometry.js';

assert.equal(axialHexDistance(0, 0, 0, 0), 0);
assert.equal(axialHexDistance(0, 0, 2, -1), 2);
assert.equal(axialHexDistance(2, -1, 0, 0), 2);

assert.equal(isInHexShape(GRID_RADIUS, GRID_RADIUS), true);
assert.equal(isInHexShape(GRID_RADIUS, 0), true);
assert.equal(isInHexShape(-1, GRID_RADIUS), false);
assert.equal(isInHexShape(GRID_RADIUS * 2, GRID_RADIUS * 2), false);

const nearby = cellsWithinHexDistance(
  GRID_RADIUS * 2 + 1,
  GRID_RADIUS * 2 + 1,
  GRID_RADIUS,
  GRID_RADIUS,
  1
);
assert.equal(nearby.size, 7);
assert.equal(nearby.has(cellKey(GRID_RADIUS + 1, GRID_RADIUS)), true);

const center = hexCenter(GRID_RADIUS, GRID_RADIUS);
assert.deepEqual(
  pixelToHex(center.cx, center.cy, GRID_RADIUS * 2 + 1, GRID_RADIUS * 2 + 1),
  { x: GRID_RADIUS, y: GRID_RADIUS }
);
assert.equal(pixelToHex(-HEX_SIZE * 10, -HEX_SIZE * 10, 11, 11), null);

const corners = hexCorners(center.cx, center.cy, HEX_SIZE);
assert.equal(corners.length, 6);
for (const [x, y] of corners) {
  assert.ok(
    Math.abs(Math.hypot(x - center.cx, y - center.cy) - HEX_SIZE) < 1e-9
  );
}

const path = [
  { x: 0, y: 0 },
  { x: 10, y: 0 },
  { x: 10, y: 30 },
];
assert.deepEqual(samplePathPixels([], 0.5), { x: 0, y: 0 });
assert.deepEqual(samplePathPixels(path, 0), path[0]);
assert.deepEqual(samplePathPixels(path, 0.25), { x: 10, y: 0 });
assert.deepEqual(samplePathPixels(path, 0.5), { x: 10, y: 10 });
assert.deepEqual(samplePathPixels(path, 1), path[2]);

console.log('grid hex geometry tests passed');
