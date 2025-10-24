//! STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.

import { type Load, newLoad, blackBox } from './load';
import { spacetimedb, type Entity, type Circle, type Food } from './schema';
import { Timestamp } from 'spacetimedb';
import { t } from 'spacetimedb/server';

function newEntity(id: number, x: number, y: number, mass: number): Entity {
  return {
    id,
    position: { x, y },
    mass,
  };
}

function newCircle(
  entity_id: number,
  player_id: number,
  x: number,
  y: number,
  magnitude: number,
  last_split_time: Timestamp
): Circle {
  return {
    entity_id,
    player_id,
    direction: { x, y },
    magnitude,
    last_split_time,
  };
}

function newFood(entity_id: number): Food {
  return {
    entity_id,
  };
}

function massToRadius(mass: number): number {
  return Math.sqrt(mass);
}

function isOverlapping(entity1: Entity, entity2: Entity): boolean {
  const entity1Radius = massToRadius(entity1.mass);
  const entity2Radius = massToRadius(entity2.mass);
  const distance = Math.sqrt(
    (entity1.position.x - entity2.position.x) ** 2 +
    (entity1.position.y - entity2.position.y) ** 2
  );
  return distance < Math.max(entity1Radius, entity2Radius);
}

// ---------- insert bulk ----------

const insertBulkEntity = spacetimedb.reducer('insert_bulk_entity', { count: t.u32() }, (ctx, { count }) => {
  for (let id = 0; id < count; id++) {
    ctx.db.entity.insert(newEntity(0, id, id + 5, id * 5));
  }
  console.info(`INSERT ENTITY: ${count}`);
});

const insertBulkCircle = spacetimedb.reducer('insert_bulk_circle', { count: t.u32() }, (ctx, { count }) => {
  for (let id = 0; id < count; id++) {
    ctx.db.circle.insert(newCircle(id, id, id, id + 5, id * 5, ctx.timestamp));
  }
  console.info(`INSERT CIRCLE: ${count}`);
});

const insertBulkFood = spacetimedb.reducer('insert_bulk_food', { count: t.u32() }, (ctx, { count }) => {
  for (let id = 1; id <= count; id++) {
    ctx.db.food.insert(newFood(id));
  }
  console.info(`INSERT FOOD: ${count}`);
});

// Simulate
// ```
// SELECT * FROM Circle, Entity, Food
// ```
const crossJoinAll = spacetimedb.reducer('cross_join_all', { expected: t.u32() }, (ctx, { expected }) => {
  let count: number = 0;

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  for (const circle of ctx.db.circle.iter()) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    for (const entity of ctx.db.entity.iter()) {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      for (const food of ctx.db.food.iter()) {
        count += 1;
      }
    }
  }

  console.info(`CROSS JOIN ALL: ${expected}, processed: ${count}`);
});

// Simulate
// ```
// SELECT * FROM Circle JOIN ENTITY USING(entity_id), Food JOIN ENTITY USING(entity_id)
// ```
const crossJoinCircleFood = spacetimedb.reducer(
  'cross_join_circle_food',
  { expected: t.u32() },
  (ctx, { expected }) => {
    let count: number = 0;

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    for (const circle of ctx.db.circle.iter()) {
      const entityId = ctx.db.entity.id;
      const circleEntity = entityId.find(circle.entity_id);
      if (circleEntity == null) {
        continue;
      }

      for (const food of ctx.db.food.iter()) {
        count += 1;

        const foodEntity = entityId.find(food.entity_id);
        if (foodEntity == null) {
          let string = JSON.stringify(circleEntity);
          throw new Error(`Entity not found: ${food.entity_id}`);
        }

        blackBox(isOverlapping(circleEntity, foodEntity));
      }
    }

    console.info(`CROSS JOIN CIRCLE FOOD: ${expected}, processed: ${count}`);
  }
);

spacetimedb.reducer(
  'init_game_circles',
  { initial_load: t.u32() },
  (ctx, { initial_load }) => {
    const load = newLoad(initial_load);

    insertBulkFood(ctx, { count: load.initialLoad });
    insertBulkEntity(ctx, { count: load.initialLoad });
    insertBulkCircle(ctx, { count: load.smallTable });
  }
);

spacetimedb.reducer(
  'run_game_circles',
  { initial_load: t.u32() },
  (ctx, { initial_load }) => {
    const load = newLoad(initial_load);

    crossJoinCircleFood(ctx, { expected: initial_load * load.smallTable });
    crossJoinAll(ctx, {
      expected: initial_load * initial_load * load.smallTable,
    });
  }
);
