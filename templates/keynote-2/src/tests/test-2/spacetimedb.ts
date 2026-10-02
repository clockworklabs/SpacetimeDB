import { makeMultiStepRecipe } from '../../scenario_recipes/multi_step_transfer.ts';

export default {
  system: 'spacetimedb',
  label: 'spacetimedb:transfer_with_audit',
  run: makeMultiStepRecipe('transfer_with_audit'),
};
