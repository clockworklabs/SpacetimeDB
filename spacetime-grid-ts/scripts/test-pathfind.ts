// Minimal sanity tests for pathfind.ts. Run: pnpm exec tsx scripts/test-pathfind.ts

import {
  type Coord,
  neighbors,
  distance,
  findPathAstar,
  dijkstra,
  coordKey,
} from '../src/math/index.ts';

let pass = 0;
let fail = 0;
function check(name: string, cond: boolean, extra?: string): void {
  if (cond) {
    pass++;
    process.stdout.write(`  ${name}  OK\n`);
  } else {
    fail++;
    process.stdout.write(`  ${name}  FAIL ${extra ?? ''}\n`);
  }
}

// Helper: build a sparse obstacle map from a 2D character grid.
//   '.' = open (cost 1)
//   '#' = wall (cost -1)
//   digits = explicit cost
function buildGrid(rows: string[]) {
  const h = rows.length;
  const w = rows[0].length;
  const obstacles = new Map<string, number>();
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const ch = rows[y][x];
      if (ch === '#') obstacles.set(coordKey({ x, y }), -1);
      else if (ch >= '2' && ch <= '9')
        obstacles.set(coordKey({ x, y }), Number(ch));
    }
  }
  const cost = (c: Coord): number => obstacles.get(coordKey(c)) ?? 1;
  const inBounds = (c: Coord) => c.x >= 0 && c.y >= 0 && c.x < w && c.y < h;
  const nb = (c: Coord) => neighbors('square', c, 4).filter(inBounds);
  return { w, h, cost, neighbors: nb };
}

// A* test cases
process.stdout.write('A* tests\n');

// 1. Trivial: start == goal
{
  const g = buildGrid(['..']);
  const r = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 0, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
  });
  check(
    'start equals goal -> found, 1 cell, 0 cost',
    r.found && r.cells.length === 1 && r.cost === 0
  );
}

// 2. Straight line, no obstacles
{
  const g = buildGrid(['.....']);
  const r = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 4, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    heuristic: (a, b) => distance('square', a, b, 4),
  });
  check(
    '5-cell straight line -> cost 4, 5 cells',
    r.found && r.cost === 4 && r.cells.length === 5
  );
}

// 3. Around a wall
{
  const g = buildGrid(['.....', '...#.', '...#.', '.....']);
  const r = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 4, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    heuristic: (a, b) => distance('square', a, b, 4),
  });
  check('straight path around no obstacle -> cost 4', r.found && r.cost === 4);
}

// 4. Goal walled off
{
  const g = buildGrid(['...#.', '...#.', '...#.', '...#.']);
  const r = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 4, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    heuristic: (a, b) => distance('square', a, b, 4),
  });
  check('goal unreachable -> found:false', !r.found);
}

// 5. Forced detour
{
  const g = buildGrid(['..#..', '..#..', '..#..', '.....']);
  const r = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 4, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    heuristic: (a, b) => distance('square', a, b, 4),
  });
  // 0,0 -> 0,3 -> 4,3 -> 4,0 = 3 + 4 + 3 = 10
  check(
    'forced detour around vertical wall -> cost 10',
    r.found && r.cost === 10,
    r.found ? `(got ${r.cost})` : ''
  );
}

// 6. Weighted terrain (swamp cost=3)
{
  const g = buildGrid(['.....', '.333.', '.333.', '.....']);
  const straight = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 4, y: 3 },
    cost: g.cost,
    neighbors: g.neighbors,
    heuristic: (a, b) => distance('square', a, b, 4),
  });
  // Cheapest is around the swamp via edges.
  check('weighted terrain finds path', straight.found);
  // 0,0 -> 4,0 -> 4,3 = cost 4 + 3 = 7 (all open).
  check(
    'weighted terrain picks cheap route (cost 7)',
    straight.found && straight.cost === 7,
    straight.found ? `(got ${straight.cost})` : ''
  );
}

// 7. Hex grid: 3 cells apart in axial coords
{
  const cost = (_c: Coord) => 1;
  const nb = (c: Coord) => neighbors('hex', c);
  const r = findPathAstar({
    start: { x: 0, y: 0 },
    goal: { x: 3, y: 0 },
    cost,
    neighbors: nb,
    heuristic: (a, b) => distance('hex', a, b),
  });
  check(
    'hex 3 cells apart -> cost 3, 4 cells',
    r.found && r.cost === 3 && r.cells.length === 4
  );
}

// Dijkstra test cases
process.stdout.write('\nDijkstra tests\n');

// 8. 5x5 open grid, range = 2 -> 13 cells (manhattan disk).
{
  const g = buildGrid(['.....', '.....', '.....', '.....', '.....']);
  const r = dijkstra({
    start: { x: 2, y: 2 },
    cost: g.cost,
    neighbors: g.neighbors,
    maxCost: 2,
  });
  check(
    'dijkstra radius 2 on open 5x5 -> 13 reachable',
    r.size === 13,
    `(got ${r.size})`
  );
}

// 9. Range 0 returns only the start.
{
  const g = buildGrid(['.']);
  const r = dijkstra({
    start: { x: 0, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    maxCost: 0,
  });
  check('dijkstra range 0 -> 1 reachable (start only)', r.size === 1);
}

// 10. Reachable record carries cost + parent.
{
  const g = buildGrid(['...', '...', '...']);
  const r = dijkstra({
    start: { x: 0, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    maxCost: 4,
  });
  const corner = r.get(coordKey({ x: 2, y: 2 }));
  check('dijkstra: corner record exists', corner !== undefined);
  check(
    'dijkstra: corner cost == 4',
    corner !== undefined && corner.cost === 4,
    `(got ${corner?.cost})`
  );
}

// 11. Wall reduces reachable set.
{
  const g = buildGrid(['...', '###', '...']);
  const r = dijkstra({
    start: { x: 0, y: 0 },
    cost: g.cost,
    neighbors: g.neighbors,
    maxCost: 100,
  });
  // Only the top row is reachable.
  check(
    'dijkstra: wall blocks lower half -> 3 reachable',
    r.size === 3,
    `(got ${r.size})`
  );
}

process.stdout.write(`\n${pass} pass, ${fail} fail\n`);
if (fail > 0) process.exit(1);
