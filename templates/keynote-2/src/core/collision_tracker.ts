export interface CollisionStats {
  total: number; // total begin() calls
  collisions: number; // how many times begin() found inflight > 0
  collisionRate: number; // collisions / total
}

export function makeCollisionTracker() {
  // how many workers are currently using each key
  const inflight = new Map<number, number>();

  let total = 0;
  let collisions = 0;

  function begin(id: number) {
    total++;
    const prev = inflight.get(id) ?? 0;
    if (prev > 0) collisions++;
    inflight.set(id, prev + 1);
  }

  function end(id: number) {
    const prev = inflight.get(id);
    if (prev === undefined) return;

    const next = prev - 1;
    if (next <= 0) inflight.delete(id);
    else inflight.set(id, next);
  }

  function stats(): CollisionStats {
    return {
      total,
      collisions,
      collisionRate: total === 0 ? 0 : collisions / total,
    };
  }

  return { begin, end, stats };
}
