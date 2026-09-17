import type { ReducerModuleCtx } from './schema.ts';

export function installFiles(_ctx: ReducerModuleCtx) {
  // Files has no scheduled jobs or singleton config. Host modules decide
  // authorization and ownership before calling the submodule helpers.
}
