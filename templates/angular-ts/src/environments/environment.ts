// Default environment — used when running `ng serve` directly (without `spacetime dev`).
// When using `npm run dev` or `spacetime dev`, the dev script generates
// environment.local.ts which replaces this file via fileReplacements in angular.json.
export const environment = {
  SPACETIMEDB_HOST: 'https://maincloud.spacetimedb.com',
  SPACETIMEDB_DB_NAME: 'angular-ts',
};
