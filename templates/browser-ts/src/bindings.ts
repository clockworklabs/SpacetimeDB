// Re-export generated bindings as globals for use in script tags
export { DbConnection } from './module_bindings';

// Make DbConnection available globally
import { DbConnection } from './module_bindings';
(window as any).DbConnection = DbConnection;
