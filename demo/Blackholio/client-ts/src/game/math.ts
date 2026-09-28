export type Vec2 = {
  x: number;
  y: number;
};

export type WeightedPosition = {
  mass: number;
  position: Vec2;
};

export function massToRadius(mass: number): number {
  return Math.sqrt(mass);
}

export function cameraSize(totalMass: number, circleCount: number): number {
  return (
    50 +
    Math.min(50, totalMass / 5) +
    Math.min(Math.max(circleCount - 1, 0), 1) * 30
  );
}

export function centerOfMass(entities: readonly WeightedPosition[]): Vec2 | undefined {
  let totalMass = 0;
  let x = 0;
  let y = 0;
  for (const entity of entities) {
    totalMass += entity.mass;
    x += entity.position.x * entity.mass;
    y += entity.position.y * entity.mass;
  }
  if (totalMass <= 0) {
    return undefined;
  }
  return { x: x / totalMass, y: y / totalMass };
}
