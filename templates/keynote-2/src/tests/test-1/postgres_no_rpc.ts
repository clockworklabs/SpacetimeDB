import { postgres_no_rpc_single_call } from '../../scenario_recipes/postgres_no_rpc_single_call.ts';

export default {
  system: 'postgres_no_rpc',
  label: 'postgres_no_rpc:single_call',
  run: postgres_no_rpc_single_call,
};
