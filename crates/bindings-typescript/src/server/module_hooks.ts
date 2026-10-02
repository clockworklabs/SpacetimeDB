import { moduleHooks, type ModuleDefaultExport } from 'spacetime:sys@2.0';
import type { UntypedSchemaDef } from '../lib/schema';
import { makeHooks } from './runtime';
import { Schema } from './schema';

// The host's entry point into a module. It is kept out of `./schema` because it
// needs the host, and a client loads `Schema` too.
declare module './schema' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars, @typescript-eslint/no-empty-object-type
  interface Schema<S extends UntypedSchemaDef> extends ModuleDefaultExport {}
}

Schema.prototype[moduleHooks] = function (exports: object) {
  return makeHooks(this.registerForHost(exports));
};
