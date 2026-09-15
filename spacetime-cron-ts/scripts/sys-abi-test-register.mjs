import { register } from 'node:module';

const loaderSource = `
  const sysAbiUrl = 'data:text/javascript,' + encodeURIComponent(
    'export function volatile_nonatomic_schedule_immediate() {}'
  );

  export function resolve(specifier, context, nextResolve) {
    if (specifier === 'spacetime:sys@2.0') {
      return { shortCircuit: true, url: sysAbiUrl };
    }
    return nextResolve(specifier, context);
  }
`;

register(
  `data:text/javascript,${encodeURIComponent(loaderSource)}`,
  import.meta.url
);
