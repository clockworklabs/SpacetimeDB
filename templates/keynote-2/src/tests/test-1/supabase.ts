import { rpc_single_call } from '../../scenario_recipes/rpc_single_call.ts';

export default {
  system: 'supabase',
  label: 'vercel+supabase:single_rpc_call',
  run: rpc_single_call,
};
