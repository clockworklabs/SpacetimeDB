export { prepareMongoDbDatabase, resetMongoDb, setMongoDbStock }
  from './backends/mongodb-operations.mjs';
export { preparePostgresDatabase, resetPostgres, setPostgresStock }
  from './backends/postgres-operations.mjs';
export { prepareSpacetimeDatabase, resetSpacetime, setSpacetimeStock }
  from './backends/spacetime-operations.mjs';

export function prepareResourceFreeDatabase({ name }) {
  return name;
}
