import type { ReducerModuleCtx } from './schema.ts';

export function installGrid(_ctx: ReducerModuleCtx) {
  // Grid owns only persistent map/entity tables. Host modules decide auth,
  // ownership, and any seed data they want layered on top.
}
