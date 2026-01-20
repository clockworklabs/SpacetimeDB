import { sql_single_statement } from '../../scenario_recipes/sql_single_statement.ts';

export default {
  system: 'cockroach',
  label: 'cockroach:single_statement',
  run: sql_single_statement,
};
