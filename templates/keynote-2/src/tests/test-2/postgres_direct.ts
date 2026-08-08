import { makeMultiStepRecipe } from '../../scenario_recipes/multi_step_transfer.ts';

// "Typical Drizzle-style usage": four separate client→PG round-trips
// (BEGIN, SELECT FOR UPDATE, UPDATE, UPDATE, INSERT audit, COMMIT).
export default {
  system: 'postgres_direct',
  label: 'postgres_direct:transfer_with_audit_steps',
  run: makeMultiStepRecipe('transfer_with_audit_steps'),
};
