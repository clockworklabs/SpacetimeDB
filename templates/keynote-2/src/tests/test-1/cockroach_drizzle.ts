import { rpc_single_call } from '../../scenario_recipes/rpc_single_call.ts';

export default {
  system: 'cockroach_drizzle',
  label: 'node+drizzle:rpc_single_call (cockroach)',
  run: rpc_single_call,
};
