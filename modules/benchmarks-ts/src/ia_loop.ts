// STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.

import { blackBox } from './load';
import { assert } from "console";
import {
  schema,
  table,
  t,
  type InferTypeOfRow,
} from 'spacetimedb/server';


const velocity = t.row("velocity", {
  entity_id: t.u32().primaryKey(),
  x: t.f32(),
  y: t.f32(),
  z: t.f32(),
});
type Velocity = InferTypeOfRow<typeof velocity>;

const position = t.row("position", {
  entity_id: t.u32().primaryKey(),
  x: t.f32(),
  y: t.f32(),
  z: t.f32(),
  vx: t.f32(),
  vy: t.f32(),
  vz: t.f32(),
});
type Position = InferTypeOfRow<typeof position>;

export const spacetimedb = schema(
  table({ name: 'velocity' }, velocity),
  table({ name: 'position' }, position),
);

function newPosition(entity_id: number, x: number, y: number, z: number): Position {
    return {
        entity_id,
        x,
        y,
        z,
        vx: x + 10.0,
        vy: y + 20.0,
        vz: z + 30.0,
    };
}

function momentMilliseconds(): bigint {
    return 1n;
}
