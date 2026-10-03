// The build of `spacetimedb/server` for a client that imports module source,
// which the `spacetimedb-client` export condition selects (see `exports` in
// package.json). It re-exports the root of `spacetimedb`, which its build
// imports rather than bundles (see tsup.config.ts), so that module source
// shares the client's classes and symbols. It adds the values of
// `spacetimedb/server` that the root lacks.
export * from '../index';
export { CaseConversionPolicy } from '../lib/autogen/types';
export { SpacetimeHostError, errors } from './errors';
export { Range } from './range';
export { Headers, Request, SyncResponse, Router } from './http';
