import { schema } from 'spacetimedb/server';
import * as inventory from './inventory';

const spacetimedb = schema({ stock: inventory });
export default spacetimedb;
