import { sql_single_statement } from '../../scenario_recipes/sql_single_statement.ts';
import sqlite from '../../connectors/direct/sqlite.ts';

export default {
  system: 'sqlite',
  label: 'sqlite:single_statement',
  connector: sqlite,
  run: sql_single_statement,
};
