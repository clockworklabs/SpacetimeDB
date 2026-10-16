import { schema } from 'spacetimedb/server';
import { file, ownerUsage, cleanup } from './model.js';
export default schema({ file, ownerUsage, cleanup });
