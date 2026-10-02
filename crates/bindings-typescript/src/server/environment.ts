import type { EnvironmentSchema } from '../lib/environment';
import type {
  EnvironmentDeclaration,
  EnvVarType,
  AlgebraicType,
} from '../lib/autogen/types';
import { OptionBuilder, StringBuilder } from '../lib/type_builders';

// These UTF-8 byte/count limits match spacetimedb_lib::environment.
const MAX_ENV_KEY_BYTES = 256;
const MAX_ENV_VALUE_BYTES = 8 * 1024;
const MAX_ENV_VARS = 256;

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
    let ty: EnvVarType;
    if (inner instanceof StringBuilder) {
      ty = { tag: 'String' };
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
      ty =
        values.length === 1
          ? { tag: 'StringLiteral', value: values[0]! }
          : { tag: 'Union', value: values };
    }
    return { name, ty, optional };
  });
}
