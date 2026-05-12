import { makeMultiStepRecipe } from '../../scenario_recipes/multi_step_transfer.ts';

// Single round-trip variant: all four steps fused into one PL/pgSQL
// stored procedure (do_transfer_with_audit). Architecturally analogous
// to a SpacetimeDB reducer (single atomic call, executed inside the
// database process).
export default {
  system: 'postgres_direct',
  label: 'postgres_direct:transfer_with_audit_storedproc',
  run: makeMultiStepRecipe('transfer_with_audit'),
};
