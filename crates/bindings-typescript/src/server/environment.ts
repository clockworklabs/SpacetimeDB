import { env_get } from 'spacetime:sys@2.2';
import type { Environment, EnvironmentSchema } from '../lib/environment';
import type {
  EnvironmentDeclaration,
  EnvironmentConstraint,
  AlgebraicType,
} from '../lib/autogen/types';
import { OptionBuilder, StringBuilder } from '../lib/type_builders';

// These UTF-8 byte/count limits match spacetimedb_lib::environment.
const MAX_ENV_KEY_BYTES = 256;
const MAX_ENV_VALUE_BYTES = 8 * 1024;
const MAX_ENV_VARS = 256;

/** Values are not cached: transaction and procedure reads retain host semantics. */
export const environment: Environment = new Proxy(
  Object.freeze(
    Object.assign(Object.create(null), { get: (key: string) => env_get(key) })
  ),
  {
    get(target, key) {
      if (key === 'get') return target.get;
      if (typeof key !== 'string') return undefined;
      // The host rejects undeclared keys and missing required values. Optional
      // named access uses undefined; the generic ABI accessor retains null.
      return env_get(key) ?? undefined;
    },
  }
);
export type {
  Environment,
  EnvironmentFor,
  EnvironmentSchema,
} from '../lib/environment';

/** Produce metadata only. No environment value is embedded in the artifact. */
export function environmentDeclarations(
  schema: EnvironmentSchema
): EnvironmentDeclaration[] {
  const entries = Object.entries(schema);
  if (entries.length > MAX_ENV_VARS)
    throw new TypeError('Too many environment declarations');
  const bytes = new TextEncoder();
  return entries.map(([name, definition]) => {
    if (
      !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ||
      bytes.encode(name).length > MAX_ENV_KEY_BYTES
    ) {
      throw new TypeError('Invalid environment declaration name');
    }
    const optional = definition instanceof OptionBuilder;
    const inner = optional ? definition.value : definition;
    let constraint: EnvironmentConstraint;
    if (inner instanceof StringBuilder) {
      constraint = { tag: 'AnyString' };
    } else {
      const type: AlgebraicType = inner?.algebraicType;
      if (type?.tag !== 'Sum' || !('variants' in inner)) {
        throw new TypeError(
          `Environment '${name}' must be a string or simple enum`
        );
      }
      const values = type.value.variants.map(variant => {
        if (
          variant.algebraicType.tag !== 'Product' ||
          variant.algebraicType.value.elements.length !== 0
        ) {
          throw new TypeError(
            `Environment '${name}' cannot use an enum payload`
          );
        }
        if (typeof variant.name !== 'string')
          throw new TypeError(
            `Environment '${name}' enum cases must have names`
          );
        if (bytes.encode(variant.name).length > MAX_ENV_VALUE_BYTES)
          throw new TypeError(`Environment '${name}' literal is too long`);
        return variant.name;
      });
      if (values.length === 0 || new Set(values).size !== values.length)
        throw new TypeError(
          `Environment '${name}' needs a nonempty literal union`
        );
      constraint =
        values.length === 1
          ? { tag: 'Literal', value: values[0]! }
          : { tag: 'OneOf', value: values };
    }
    return { name, constraint, optional };
  });
}
