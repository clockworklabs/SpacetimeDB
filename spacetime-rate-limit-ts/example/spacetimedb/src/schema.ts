import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import {
  schema,
  table,
  t,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import {
  rateLimitEvent,
  reactorEvent,
  reactorPlayerState,
  reactorRoomState,
} from './model';

export {
  rateLimitEvent,
  reactorEvent,
  reactorPlayerState,
  reactorRoomState,
} from './model';

export const rateLimitDemoConfig = table(
  { name: 'rate_limit_demo_config', public: true },
  {
    singleton: t.bool().primaryKey(),
    retainEvents: t.u32(),
    eventPruneBatch: t.u32(),
    updatedAt: t.timestamp(),
  }
);

let sweepReducer: unknown;

export const rateLimitDemoSweepTick = table(
  {
    name: 'rate_limit_demo_sweep_tick',
    scheduled: (): any => {
      if (!sweepReducer) {
        throw new Error('rate_limit_demo.sweep_reducer_not_registered');
      }
      return sweepReducer;
    },
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

export function setSweepReducer(reducer: unknown): void {
  sweepReducer = reducer;
}

export const spacetimedb = schema({
  rateLimit,
  rateLimitEvent,
  reactorRoomState,
  reactorPlayerState,
  reactorEvent,
  rateLimitDemoConfig,
  rateLimitDemoSweepTick,
});

export type Schema = InferSchema<typeof spacetimedb>;
export type Tx = ReducerCtx<Schema>;
