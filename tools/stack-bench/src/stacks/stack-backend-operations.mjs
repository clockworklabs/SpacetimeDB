export { prepareMongoDbDatabase, proveMongoDbUse, resetMongoDb, setMongoDbStock }
  from './backends/mongodb-operations.mjs';
export { preparePostgresDatabase, provePostgresUse, resetPostgres, setPostgresStock }
  from './backends/postgres-operations.mjs';
export { prepareSpacetimeDatabase, resetSpacetime, setSpacetimeStock }
  from './backends/spacetime-operations.mjs';

export function prepareResourceFreeDatabase({ name }) {
  return name;
}
