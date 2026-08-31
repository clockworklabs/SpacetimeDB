export { prepareMongoDbDatabase, proveMongoDbUse, resetMongoDb, setMongoDbStock }
  from './backends/mongodb-operations.js';
export { preparePostgresDatabase, provePostgresUse, resetPostgres, setPostgresStock }
  from './backends/postgres-operations.js';
export { prepareSpacetimeDatabase, resetSpacetime, setSpacetimeStock }
  from './backends/spacetime-operations.js';

export function prepareResourceFreeDatabase({ name }: { name: string }): string {
  return name;
}
