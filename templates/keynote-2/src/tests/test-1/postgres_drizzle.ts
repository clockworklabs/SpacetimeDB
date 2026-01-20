import { rpc_single_call } from '../../scenario_recipes/rpc_single_call.ts';

export default {
  system: 'postgres_drizzle',
  label: 'node+drizzle:rpc_single_call (postgres)',
  run: rpc_single_call,
};
