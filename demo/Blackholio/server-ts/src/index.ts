import { ScheduleAt } from 'spacetimedb';
import {
  schema,
  table,
  t,
  type InferSchema,
  type InferTypeOfRow,
  type ReducerCtx,
} from 'spacetimedb/server';
import {
  add,
  DbVector2,
  magnitude,
  mul,
  normalized,
  sqrMagnitude,
  sub,
  vec,
  type DbVector2 as DbVector2Type,
} from './math';

const START_PLAYER_MASS = 15;
const START_PLAYER_SPEED = 10;
const FOOD_MASS_MIN = 2;
const FOOD_MASS_MAX = 4;
const TARGET_FOOD_COUNT = 600n;
const MINIMUM_SAFE_MASS_RATIO = 0.85;

const MIN_MASS_TO_SPLIT = START_PLAYER_MASS * 2;
const MAX_CIRCLES_PER_PLAYER = 16;
const SPLIT_RECOMBINE_DELAY_SEC = 5;
const SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC = 2;
const ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT = 0.9;
const SELF_COLLISION_SPEED = 0.05;

const MICROS_PER_SECOND = 1_000_000n;

const configRow = t.row('Config', {
  id: t.i32().primaryKey(),
  world_size: t.i64().name('world_size'),
});

const entityRow = t.row('Entity', {
  entity_id: t.i32().primaryKey().autoInc().name('entity_id'),
  position: DbVector2,
  mass: t.i32(),
});
type Entity = InferTypeOfRow<typeof entityRow.row>;

const circleRow = t.row('Circle', {
  entity_id: t.i32().primaryKey().name('entity_id'),
  player_id: t.i32().index().name('player_id'),
  direction: DbVector2,
  speed: t.f32(),
  last_split_time: t.timestamp().name('last_split_time'),
});
type Circle = InferTypeOfRow<typeof circleRow.row>;

const playerRow = t.row('Player', {
  identity: t.identity().primaryKey(),
  player_id: t.i32().unique().autoInc().name('player_id'),
  name: t.string(),
});
type Player = InferTypeOfRow<typeof playerRow.row>;

const foodRow = t.row('Food', {
  entity_id: t.i32().primaryKey().name('entity_id'),
});

const moveAllPlayersTimerRow = t.row('MoveAllPlayersTimer', {
  scheduled_id: t.u64().primaryKey().autoInc().name('scheduled_id'),
  scheduled_at: t.scheduleAt().name('scheduled_at'),
});
type MoveAllPlayersTimer = InferTypeOfRow<typeof moveAllPlayersTimerRow.row>;

const spawnFoodTimerRow = t.row('SpawnFoodTimer', {
  scheduled_id: t.u64().primaryKey().autoInc().name('scheduled_id'),
  scheduled_at: t.scheduleAt().name('scheduled_at'),
});
type SpawnFoodTimer = InferTypeOfRow<typeof spawnFoodTimerRow.row>;

const circleDecayTimerRow = t.row('CircleDecayTimer', {
  scheduled_id: t.u64().primaryKey().autoInc().name('scheduled_id'),
  scheduled_at: t.scheduleAt().name('scheduled_at'),
});
type CircleDecayTimer = InferTypeOfRow<typeof circleDecayTimerRow.row>;

const circleRecombineTimerRow = t.row('CircleRecombineTimer', {
  scheduled_id: t.u64().primaryKey().autoInc().name('scheduled_id'),
  scheduled_at: t.scheduleAt().name('scheduled_at'),
  player_id: t.i32().name('player_id'),
});
type CircleRecombineTimer = InferTypeOfRow<typeof circleRecombineTimerRow.row>;

const consumeEntityEventRow = t.row('ConsumeEntityEvent', {
  consumed_entity_id: t.i32().name('consumed_entity_id'),
  consumer_entity_id: t.i32().name('consumer_entity_id'),
});

const consumeEntityTimerRow = t.row('ConsumeEntityTimer', {
  scheduled_id: t.u64().primaryKey().autoInc().name('scheduled_id'),
  scheduled_at: t.scheduleAt().name('scheduled_at'),
  consumed_entity_id: t.i32().name('consumed_entity_id'),
  consumer_entity_id: t.i32().name('consumer_entity_id'),
});
type ConsumeEntityTimer = InferTypeOfRow<typeof consumeEntityTimerRow.row>;

const spacetimedb = schema({
  config: table({ public: true }, configRow),
  entity: table({ public: true }, entityRow),
  circle: table({ public: true }, circleRow),
  player: table({ public: true }, playerRow),
  food: table({ public: true }, foodRow),
  consume_entity_event: table(
    { public: true, event: true },
    consumeEntityEventRow
  ),
  logged_out_entity: table({ name: 'logged_out_entity' }, entityRow),
  logged_out_circle: table({ name: 'logged_out_circle' }, circleRow),
  logged_out_player: table({ name: 'logged_out_player' }, playerRow),
  move_all_players_timer: table(
    {
      name: 'move_all_players_timer',
      scheduled: (): any => move_all_players,
    },
    moveAllPlayersTimerRow
  ),
  spawn_food_timer: table(
    {
      name: 'spawn_food_timer',
      scheduled: (): any => spawn_food,
    },
    spawnFoodTimerRow
  ),
  circle_decay_timer: table(
    {
      name: 'circle_decay_timer',
      scheduled: (): any => circle_decay,
    },
    circleDecayTimerRow
  ),
  circle_recombine_timer: table(
    {
      name: 'circle_recombine_timer',
      scheduled: (): any => circle_recombine,
    },
    circleRecombineTimerRow
  ),
  consume_entity_timer: table(
    {
      name: 'consume_entity_timer',
      scheduled: (): any => consume_entity,
    },
    consumeEntityTimerRow
  ),
});
export default spacetimedb;

type BlackholioCtx = ReducerCtx<InferSchema<typeof spacetimedb>>;

export const init = spacetimedb.init(ctx => {
  ctx.db.config.insert({ id: 0, world_size: 1000n });
  ctx.db.circle_decay_timer.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(5n * MICROS_PER_SECOND),
  });
  ctx.db.spawn_food_timer.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(500_000n),
  });
  ctx.db.move_all_players_timer.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(50_000n),
  });
});

export const connect = spacetimedb.clientConnected(ctx => {
  const loggedOutPlayer = ctx.db.logged_out_player.identity.find(ctx.sender);
  if (loggedOutPlayer) {
    ctx.db.player.insert(loggedOutPlayer);
    ctx.db.logged_out_player.identity.delete(loggedOutPlayer.identity);

    for (const circle of ctx.db.logged_out_circle.player_id.filter(
      loggedOutPlayer.player_id
    )) {
      ctx.db.logged_out_circle.entity_id.delete(circle.entity_id);
      ctx.db.circle.insert(circle);
      const entity = ctx.db.logged_out_entity.entity_id.find(circle.entity_id);
      if (!entity) {
        throw new Error('Logged out circle has no entity');
      }
      ctx.db.logged_out_entity.entity_id.delete(circle.entity_id);
      ctx.db.entity.insert(entity);
    }
  } else {
    ctx.db.player.insert({
      identity: ctx.sender,
      player_id: 0,
      name: '',
    });
  }
});

export const disconnect = spacetimedb.clientDisconnected(ctx => {
  const player = ctx.db.player.identity.find(ctx.sender);
  if (!player) {
    throw new Error('Player not found');
  }
  const player_id = player.player_id;
  ctx.db.logged_out_player.insert(player);
  ctx.db.player.identity.delete(ctx.sender);

  for (const circle of ctx.db.circle.player_id.filter(player_id)) {
    const entity = ctx.db.entity.entity_id.find(circle.entity_id);
    if (!entity) {
      throw new Error('Circle has no entity');
    }
    ctx.db.logged_out_entity.insert(entity);
    ctx.db.entity.entity_id.delete(circle.entity_id);
    ctx.db.logged_out_circle.insert(circle);
    ctx.db.circle.entity_id.delete(circle.entity_id);
  }
});

export const enter_game = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    console.info(`Creating player with name ${name}`);
    const player = ctx.db.player.identity.find(ctx.sender);
    if (!player) {
      throw new Error('');
    }
    ctx.db.player.identity.update({ ...player, name });
    spawnPlayerInitialCircle(ctx, player.player_id);
  }
);

export const respawn = spacetimedb.reducer(ctx => {
  const player = ctx.db.player.identity.find(ctx.sender);
  if (!player) {
    throw new Error('No such player found');
  }
  spawnPlayerInitialCircle(ctx, player.player_id);
});

export const suicide = spacetimedb.reducer(ctx => {
  const player = ctx.db.player.identity.find(ctx.sender);
  if (!player) {
    throw new Error('No such player found');
  }
  for (const circle of ctx.db.circle.player_id.filter(player.player_id)) {
    destroyEntity(ctx, circle.entity_id);
  }
});

export const update_player_input = spacetimedb.reducer(
  { direction: DbVector2 },
  (ctx, { direction }) => {
    const player = ctx.db.player.identity.find(ctx.sender);
    if (!player) {
      throw new Error('Player not found');
    }
    for (const circle of ctx.db.circle.player_id.filter(player.player_id)) {
      const inputMagnitude = magnitude(direction);
      ctx.db.circle.entity_id.update({
        ...circle,
        direction: normalized(direction),
        speed: Math.min(1, Math.max(0, inputMagnitude)),
      });
    }
  }
);

export const move_all_players = spacetimedb.reducer(
  { arg: moveAllPlayersTimerRow },
  (ctx, { arg: _timer }: { arg: MoveAllPlayersTimer }) => {
    const config = ctx.db.config.id.find(0);
    if (!config) {
      throw new Error('Config not found');
    }
    const world_size = Number(config.world_size);
    const circleDirections = new Map<number, DbVector2Type>();
    for (const circle of ctx.db.circle.iter()) {
      circleDirections.set(circle.entity_id, mul(circle.direction, circle.speed));
    }

    for (const player of ctx.db.player.iter()) {
      const circles = Array.from(ctx.db.circle.player_id.filter(player.player_id));
      const playerEntities = circles.map(circle => {
        const entity = ctx.db.entity.entity_id.find(circle.entity_id);
        if (!entity) {
          throw new Error('Circle has no entity');
        }
        return { ...entity };
      });
      if (playerEntities.length <= 1) {
        continue;
      }
      applySplitMovement(
        ctx.timestamp.microsSinceUnixEpoch,
        circles,
        playerEntities,
        circleDirections
      );
    }

    for (const circle of ctx.db.circle.iter()) {
      const circleEntity = ctx.db.entity.entity_id.find(circle.entity_id);
      if (!circleEntity) {
        continue;
      }
      const circleRadius = massToRadius(circleEntity.mass);
      const direction = circleDirections.get(circle.entity_id);
      if (!direction) {
        continue;
      }
      const newPos = add(
        circleEntity.position,
        mul(direction, massToMaxMoveSpeed(circleEntity.mass))
      );
      const min = circleRadius;
      const max = world_size - circleRadius;
      ctx.db.entity.entity_id.update({
        ...circleEntity,
        position: {
          x: clamp(newPos.x, min, max),
          y: clamp(newPos.y, min, max),
        },
      });
    }

    const entities = new Map<number, Entity>();
    for (const entity of ctx.db.entity.iter()) {
      entities.set(entity.entity_id, entity);
    }
    for (const circle of ctx.db.circle.iter()) {
      const circleEntity = entities.get(circle.entity_id);
      if (!circleEntity) {
        continue;
      }
      for (const otherEntity of entities.values()) {
        if (otherEntity.entity_id === circleEntity.entity_id) {
          continue;
        }
        if (!isOverlapping(circleEntity, otherEntity)) {
          continue;
        }
        const otherCircle = ctx.db.circle.entity_id.find(otherEntity.entity_id);
        if (otherCircle) {
          if (otherCircle.player_id !== circle.player_id) {
            const massRatio = otherEntity.mass / circleEntity.mass;
            if (massRatio < MINIMUM_SAFE_MASS_RATIO) {
              scheduleConsumeEntity(
                ctx,
                circleEntity.entity_id,
                otherEntity.entity_id
              );
            }
          }
        } else {
          scheduleConsumeEntity(ctx, circleEntity.entity_id, otherEntity.entity_id);
        }
      }
    }
  }
);

export const consume_entity = spacetimedb.reducer(
  { arg: consumeEntityTimerRow },
  (ctx, { arg }: { arg: ConsumeEntityTimer }) => {
    const consumedEntity = ctx.db.entity.entity_id.find(arg.consumed_entity_id);
    const consumerEntity = ctx.db.entity.entity_id.find(arg.consumer_entity_id);
    if (!consumedEntity) {
      throw new Error("Consumed entity doesn't exist");
    }
    if (!consumerEntity) {
      throw new Error("Consumer entity doesn't exist");
    }
    ctx.db.consume_entity_event.insert({
      consumed_entity_id: consumedEntity.entity_id,
      consumer_entity_id: consumerEntity.entity_id,
    });
    destroyEntity(ctx, consumedEntity.entity_id);
    ctx.db.entity.entity_id.update({
      ...consumerEntity,
      mass: consumerEntity.mass + consumedEntity.mass,
    });
  }
);

export const player_split = spacetimedb.reducer(ctx => {
  const player = ctx.db.player.identity.find(ctx.sender);
  if (!player) {
    throw new Error('Sender has no player');
  }
  const circles = Array.from(ctx.db.circle.player_id.filter(player.player_id));
  let circleCount = circles.length;
  if (circleCount >= MAX_CIRCLES_PER_PLAYER) {
    return;
  }

  for (const circle of circles) {
    const circleEntity = ctx.db.entity.entity_id.find(circle.entity_id);
    if (!circleEntity) {
      throw new Error('Circle has no entity');
    }
    if (circleEntity.mass >= MIN_MASS_TO_SPLIT * 2) {
      const halfMass = Math.trunc(circleEntity.mass / 2);
      spawnCircleAt(
        ctx,
        circle.player_id,
        halfMass,
        add(circleEntity.position, circle.direction),
        ctx.timestamp
      );
      ctx.db.entity.entity_id.update({
        ...circleEntity,
        mass: circleEntity.mass - halfMass,
      });
      ctx.db.circle.entity_id.update({
        ...circle,
        last_split_time: ctx.timestamp,
      });
      circleCount += 1;
      if (circleCount >= MAX_CIRCLES_PER_PLAYER) {
        break;
      }
    }
  }

  ctx.db.circle_recombine_timer.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.time(
      ctx.timestamp.microsSinceUnixEpoch +
        BigInt(SPLIT_RECOMBINE_DELAY_SEC) * MICROS_PER_SECOND
    ),
    player_id: player.player_id,
  });

  console.warn('Player split!');
});

export const spawn_food = spacetimedb.reducer(
  { arg: spawnFoodTimerRow },
  (ctx, { arg: _timer }: { arg: SpawnFoodTimer }) => {
    if (ctx.db.player.count() === 0n) {
      return;
    }
    const config = ctx.db.config.id.find(0);
    if (!config) {
      throw new Error('Config not found');
    }
    const world_size = Number(config.world_size);
    let foodCount = ctx.db.food.count();
    while (foodCount < TARGET_FOOD_COUNT) {
      const foodMass = ctx.random.integerInRange(
        FOOD_MASS_MIN,
        FOOD_MASS_MAX - 1
      );
      const foodRadius = massToRadius(foodMass);
      const entity = ctx.db.entity.insert({
        entity_id: 0,
        position: {
          x: randomRange(ctx.random, foodRadius, world_size - foodRadius),
          y: randomRange(ctx.random, foodRadius, world_size - foodRadius),
        },
        mass: foodMass,
      });
      ctx.db.food.insert({ entity_id: entity.entity_id });
      foodCount += 1n;
      console.info(`Spawned food! ${entity.entity_id}`);
    }
  }
);

export const circle_decay = spacetimedb.reducer(
  { arg: circleDecayTimerRow },
  (ctx, { arg: _timer }: { arg: CircleDecayTimer }) => {
    for (const circle of ctx.db.circle.iter()) {
      const circleEntity = ctx.db.entity.entity_id.find(circle.entity_id);
      if (!circleEntity) {
        throw new Error('Entity not found');
      }
      if (circleEntity.mass <= START_PLAYER_MASS) {
        continue;
      }
      ctx.db.entity.entity_id.update({
        ...circleEntity,
        mass: Math.trunc(circleEntity.mass * 0.99),
      });
    }
  }
);

export const circle_recombine = spacetimedb.reducer(
  { arg: circleRecombineTimerRow },
  (ctx, { arg }: { arg: CircleRecombineTimer }) => {
    const circles = Array.from(ctx.db.circle.player_id.filter(arg.player_id));
    const recombiningEntities = circles
      .filter(
        circle =>
          secondsSince(
            ctx.timestamp.microsSinceUnixEpoch,
            circle.last_split_time.microsSinceUnixEpoch
          ) >= SPLIT_RECOMBINE_DELAY_SEC
      )
      .map(circle => {
        const entity = ctx.db.entity.entity_id.find(circle.entity_id);
        if (!entity) {
          throw new Error('Circle has no entity');
        }
        return entity;
      });
    if (recombiningEntities.length <= 1) {
      return;
    }
    const baseEntityId = recombiningEntities[0].entity_id;
    for (let i = 1; i < recombiningEntities.length; i++) {
      scheduleConsumeEntity(
        ctx,
        baseEntityId,
        recombiningEntities[i].entity_id
      );
    }
  }
);

function spawnPlayerInitialCircle(
  ctx: BlackholioCtx,
  player_id: number
): Entity {
  const config = ctx.db.config.id.find(0);
  if (!config) {
    throw new Error('Config not found');
  }
  const world_size = Number(config.world_size);
  const playerStartRadius = massToRadius(START_PLAYER_MASS);
  return spawnCircleAt(
    ctx,
    player_id,
    START_PLAYER_MASS,
    {
      x: randomRange(
        ctx.random,
        playerStartRadius,
        world_size - playerStartRadius
      ),
      y: randomRange(
        ctx.random,
        playerStartRadius,
        world_size - playerStartRadius
      ),
    },
    ctx.timestamp
  );
}

function spawnCircleAt(
  ctx: BlackholioCtx,
  player_id: number,
  mass: number,
  position: DbVector2Type,
  timestamp: BlackholioCtx['timestamp']
): Entity {
  const entity = ctx.db.entity.insert({
    entity_id: 0,
    position,
    mass,
  });
  ctx.db.circle.insert({
    entity_id: entity.entity_id,
    player_id,
    direction: vec(0, 1),
    speed: 0,
    last_split_time: timestamp,
  });
  return entity;
}

function destroyEntity(ctx: BlackholioCtx, entity_id: number): void {
  ctx.db.food.entity_id.delete(entity_id);
  ctx.db.circle.entity_id.delete(entity_id);
  ctx.db.entity.entity_id.delete(entity_id);
}

function scheduleConsumeEntity(
  ctx: BlackholioCtx,
  consumer_entity_id: number,
  consumed_entity_id: number
): void {
  ctx.db.consume_entity_timer.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch),
    consumer_entity_id,
    consumed_entity_id,
  });
}

function applySplitMovement(
  nowMicros: bigint,
  circles: Circle[],
  playerEntities: Entity[],
  circleDirections: Map<number, DbVector2Type>
): void {
  const count = playerEntities.length;
  for (let i = 0; i < playerEntities.length; i++) {
    const circleI = circles[i];
    const timeSinceSplit = secondsSince(
      nowMicros,
      circleI.last_split_time.microsSinceUnixEpoch
    );
    const timeBeforeRecombining = Math.max(
      0,
      SPLIT_RECOMBINE_DELAY_SEC - timeSinceSplit
    );
    if (timeBeforeRecombining > SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC) {
      continue;
    }
    const entityI = playerEntities[i];
    for (let j = 0; j < playerEntities.length; j++) {
      if (i === j) {
        continue;
      }
      const entityJ = playerEntities[j];
      let diff = sub(entityI.position, entityJ.position);
      let distanceSqr = sqrMagnitude(diff);
      if (distanceSqr <= 0.0001) {
        diff = vec(1, 0);
        distanceSqr = 1;
      }
      const radiusSum = massToRadius(entityI.mass) + massToRadius(entityJ.mass);
      if (distanceSqr > radiusSum * radiusSum) {
        const gravityMultiplier =
          1 -
          timeBeforeRecombining / SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC;
        const adjustment = mul(
          normalized(diff),
          ((radiusSum - Math.sqrt(distanceSqr)) * gravityMultiplier * 0.05) /
            count
        );
        addDirection(
          circleDirections,
          entityI.entity_id,
          mul(adjustment, 0.5)
        );
        addDirection(
          circleDirections,
          entityJ.entity_id,
          mul(adjustment, -0.5)
        );
      }
    }
  }

  for (let i = 0; i < playerEntities.length; i++) {
    const entityI = playerEntities[i];
    for (let j = i + 1; j < playerEntities.length; j++) {
      const entityJ = playerEntities[j];
      let diff = sub(entityI.position, entityJ.position);
      let distanceSqr = sqrMagnitude(diff);
      if (distanceSqr <= 0.0001) {
        diff = vec(1, 0);
        distanceSqr = 1;
      }
      const radiusSum = massToRadius(entityI.mass) + massToRadius(entityJ.mass);
      const radiusSumMultiplied = radiusSum * ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT;
      if (distanceSqr < radiusSumMultiplied * radiusSumMultiplied) {
        const adjustment = mul(
          normalized(diff),
          (radiusSum - Math.sqrt(distanceSqr)) * SELF_COLLISION_SPEED
        );
        addDirection(
          circleDirections,
          entityI.entity_id,
          mul(adjustment, 0.5)
        );
        addDirection(
          circleDirections,
          entityJ.entity_id,
          mul(adjustment, -0.5)
        );
      }
    }
  }
}

function addDirection(
  directions: Map<number, DbVector2Type>,
  entity_id: number,
  delta: DbVector2Type
): void {
  directions.set(entity_id, add(directions.get(entity_id) ?? vec(0, 0), delta));
}

function isOverlapping(a: Entity, b: Entity): boolean {
  const dx = a.position.x - b.position.x;
  const dy = a.position.y - b.position.y;
  const distanceSq = dx * dx + dy * dy;
  const maxRadius = Math.max(massToRadius(a.mass), massToRadius(b.mass));
  return distanceSq <= maxRadius * maxRadius;
}

function massToRadius(mass: number): number {
  return Math.sqrt(mass);
}

function massToMaxMoveSpeed(mass: number): number {
  return (2 * START_PLAYER_SPEED) / (1 + Math.sqrt(mass / START_PLAYER_MASS));
}

function secondsSince(nowMicros: bigint, thenMicros: bigint): number {
  return Number(nowMicros - thenMicros) / Number(MICROS_PER_SECOND);
}

function randomRange(
  random: { (): number },
  minInclusive: number,
  maxExclusive: number
): number {
  return minInclusive + random() * (maxExclusive - minInclusive);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
