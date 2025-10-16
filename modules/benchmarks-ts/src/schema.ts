import {
  schema,
  table,
  t,
  type Infer,
  type InferTypeOfRow,
  type InferSchema,
} from 'spacetimedb/server';

// circles:
// -----------------------------------------------------------------------------

const vector2 = t.object('Vector2', {
  x: t.f32(),
  y: t.f32(),
});
export type Vector2 = Infer<typeof vector2>;

const entityRow = t.row({
  id: t.u32().primaryKey().autoInc(),
  position: vector2,
  mass: t.u32(),
});
export type Entity = Infer<typeof entityRow>;

const circleRow = t.row({
  entity_id: t.u32().primaryKey(),
  player_id: t.u32().index('btree'),
  direction: vector2,
  magnitude: t.f32(),
  last_split_time: t.timestamp(),
});
export type Circle = Infer<typeof circleRow>;

const foodRow = t.row({
  entity_id: t.u32().primaryKey(),
});
export type Food = Infer<typeof foodRow>;

const entityTable = table({ name: 'entity' }, entityRow);
const circleTable = table({ name: 'circle' }, circleRow);
const foodTable = table({ name: 'food' }, foodRow);

// synthetic:
// -----------------------------------------------------------------------------

export const unique_0_u32_u64_str_tRow = t.row({
  id: t.u32().unique(),
  age: t.u64(),
  name: t.string(),
});

export const no_index_u32_u64_str_tRow = t.row({
  id: t.u32(),
  age: t.u64(),
  name: t.string(),
});

export const btree_each_column_u32_u64_str_tRow = t.row({
  id: t.u32().index('btree'),
  age: t.u64().index('btree'),
  name: t.string().index('btree'),
});

export const unique_0_u32_u64_u64_tRow = t.row({
  id: t.u32().unique(),
  x: t.u64(),
  y: t.u64(),
});

export const no_index_u32_u64_u64_tRow = t.row({
  id: t.u32(),
  x: t.u64(),
  y: t.u64(),
});

export const btree_each_column_u32_u64_u64_tRow = t.row({
  id: t.u32().index('btree'),
  x: t.u64().index('btree'),
  y: t.u64().index('btree'),
});

const unique_0_u32_u64_strTable = table(
  { name: 'unique_0_u32_u64_str' },
  unique_0_u32_u64_str_tRow
);
const no_index_u32_u64_strTable = table(
  { name: 'no_index_u32_u64_str' },
  no_index_u32_u64_str_tRow
);
const btree_each_column_u32_u64_strTable = table(
  { name: 'btree_each_column_u32_u64_str' },
  btree_each_column_u32_u64_str_tRow
);
const unique_0_u32_u64_u64Table = table(
  { name: 'unique_0_u32_u64_u64' },
  unique_0_u32_u64_u64_tRow
);
const no_index_u32_u64_u64Table = table(
  { name: 'no_index_u32_u64_u64' },
  no_index_u32_u64_u64_tRow
);
const btree_each_column_u32_u64_u64Table = table(
  { name: 'btree_each_column_u32_u64_u64' },
  btree_each_column_u32_u64_u64_tRow
);

// ia_loop:
// -----------------------------------------------------------------------------

const velocity = t.row({
  entity_id: t.u32().primaryKey(),
  x: t.f32(),
  y: t.f32(),
  z: t.f32(),
});
export type Velocity = Infer<typeof velocity>;

const position = t.row({
  entity_id: t.u32().primaryKey(),
  x: t.f32(),
  y: t.f32(),
  z: t.f32(),
  vx: t.f32(),
  vy: t.f32(),
  vz: t.f32(),
});
export type Position = Infer<typeof position>;

const agentAction = t.enum('AgentAction', {
  Inactive: t.unit(),
  Idle: t.unit(),
  Evading: t.unit(),
  Investigating: t.unit(),
  Retreating: t.unit(),
  Fighting: t.unit(),
});
export type AgentAction = Infer<typeof agentAction>;

const gameEnemyAiAgentState = t.row({
  entity_id: t.u64().primaryKey(),
  last_move_timestamps: t.array(t.u64()),
  next_action_timestamp: t.u64(),
  action: agentAction,
});
export type GameEnemyAiAgentState = Infer<typeof gameEnemyAiAgentState>;

const gameTargetableState = t.row({
  entity_id: t.u64().primaryKey(),
  quad: t.i64(),
});
export type GameTargetableState = Infer<typeof gameTargetableState>;

const gameLiveTargetableState = t.row({
  entity_id: t.u64().unique(),
  quad: t.i64().index('btree'),
});
export type GameLiveTargetableState = Infer<typeof gameLiveTargetableState>;

const gameMobileEntityState = t.row({
  entity_id: t.u64().primaryKey(),
  location_x: t.i32().index('btree'),
  location_y: t.i32(),
  timestamp: t.u64(),
});
export type GameMobileEntityState = Infer<typeof gameMobileEntityState>;

const gameEnemyState = t.row({
  entity_id: t.u64().primaryKey(),
  herd_id: t.i32(),
});
export type GameEnemyState = Infer<typeof gameEnemyState>;

const smallHexTile = t.object('SmallHexTile', {
  x: t.i32(),
  z: t.i32(),
  dimension: t.u32(),
});
export type SmallHexTile = Infer<typeof smallHexTile>;

const gameHerdCache = t.row({
  id: t.i32().primaryKey(),
  dimension_id: t.u32(),
  current_population: t.i32(),
  location: smallHexTile,
  max_population: t.i32(),
  spawn_eagerness: t.f32(),
  roaming_distance: t.i32(),
});
export type GameHerdCache = Infer<typeof gameHerdCache>;

const velocityTable = table({ name: 'velocity' }, velocity);
const positionTable = table({ name: 'position' }, position);
const gameEnemyAiAgentStateTable = table(
  {
    name: 'game_enemy_ai_agent_state',
  },
  gameEnemyAiAgentState
);
const gameTargetableStateTable = table(
  {
    name: 'game_targetable_state',
  },
  gameTargetableState
);
const gameLiveTargetableStateTable = table(
  {
    name: 'game_live_targetable_state',
  },
  gameLiveTargetableState
);
const gameEnemyStateTable = table(
  {
    name: 'game_enemy_state',
  },
  gameEnemyState
);
const gameHerdCacheTable = table(
  {
    name: 'game_herd_cache',
  },
  gameHerdCache
);

export const spacetimedb = schema(
  circleTable,
  entityTable,
  foodTable,
  unique_0_u32_u64_strTable,
  no_index_u32_u64_strTable,
  btree_each_column_u32_u64_strTable,
  unique_0_u32_u64_u64Table,
  no_index_u32_u64_u64Table,
  btree_each_column_u32_u64_u64Table,
  velocityTable,
  positionTable,
  gameEnemyAiAgentStateTable,
  gameTargetableStateTable,
  gameLiveTargetableStateTable,
  gameEnemyStateTable,
  gameHerdCacheTable,
  table({ name: 'game_mobile_entity_state' }, gameMobileEntityState)
);

export type S = InferSchema<typeof spacetimedb>;
