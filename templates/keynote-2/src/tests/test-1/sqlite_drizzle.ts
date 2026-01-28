import { rpc_single_call } from '../../scenario_recipes/rpc_single_call.ts';

export default {
  system: 'sqlite_drizzle',
  label: 'node+drizzle:rpc_single_call (sqlite)',
  run: rpc_single_call,
};
