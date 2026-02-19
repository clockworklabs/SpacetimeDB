// Re-export generated bindings as globals for use in script tags
export { DbConnection, tables } from './module_bindings';

// Make generated bindings available globally
import { DbConnection, tables } from './module_bindings';
(window as any).DbConnection = DbConnection;
(window as any).tables = tables;
