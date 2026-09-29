import { register } from 'node:module';

// Host syscall modules exist only inside SpacetimeDB. Stub the bindings that
// `spacetimedb/server` reads while loading.
const stubs = {
  'spacetime:sys@2.0':
    'export const moduleHooks = Symbol(); export function row_iter_bsatn_close() {} export function volatile_nonatomic_schedule_immediate() {}',
  'spacetime:sys@2.1': 'export {};',
  'spacetime:sys@2.2': 'export function env_get() {}',
};

const loaderSource = `
  const stubs = ${JSON.stringify(stubs)};
  export function resolve(specifier, context, nextResolve) {
    return specifier in stubs
      ? {
          shortCircuit: true,
          url: 'data:text/javascript,' + encodeURIComponent(stubs[specifier]),
        }
      : nextResolve(specifier, context);
  }
`;

register(
  `data:text/javascript,${encodeURIComponent(loaderSource)}`,
  import.meta.url
);
