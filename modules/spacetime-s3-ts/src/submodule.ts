import { schema } from 'spacetimedb/server';
import { file, ownerUsage, cleanup } from './model.js';

const tables: {
  file: typeof file;
  ownerUsage: typeof ownerUsage;
  cleanup: typeof cleanup;
} = { file, ownerUsage, cleanup };
const spacetimedb: ReturnType<typeof schema<typeof tables>> = schema(tables);
export default spacetimedb;
