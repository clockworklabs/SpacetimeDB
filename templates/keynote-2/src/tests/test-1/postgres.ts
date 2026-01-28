import { sql_single_statement } from '../../scenario_recipes/sql_single_statement.ts';

export default {
  system: 'postgres',
  label: 'postgres / node+postgres:single_statement',
  run: sql_single_statement,
};
