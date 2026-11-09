import { schema } from 'spacetimedb/server';
import { sandbox, execution } from './model.js';

const tables: { sandbox: typeof sandbox; execution: typeof execution } = {
  sandbox,
  execution,
};
const spacetimedb: ReturnType<typeof schema<typeof tables>> = schema(tables);
export default spacetimedb;
