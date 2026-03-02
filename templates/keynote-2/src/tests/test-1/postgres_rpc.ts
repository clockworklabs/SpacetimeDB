import { rpc_single_call } from '../../scenario_recipes/rpc_single_call.ts';

export default {
  system: 'postgres_rpc',
  label: 'postgres_rpc:rpc_single_call',
  run: rpc_single_call,
};
