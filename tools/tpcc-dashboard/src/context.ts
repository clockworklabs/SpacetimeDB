import { createContext } from 'react';
import type { DbConnection } from './module_bindings';

export const SpacetimeDBContext = createContext<DbConnection | null>(null);
