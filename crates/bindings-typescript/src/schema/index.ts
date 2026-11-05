/** Client module schemas, sharing builder identities with the root SDK. */
export {
  moduleSchema as schema,
  ModuleSchema as Schema,
  FunctionSchema,
  t,
  table,
  CaseConversionPolicy,
} from 'spacetimedb';
export type {
  ModuleSchemaExports,
  SchemaEntry,
  SchemaEntries,
  FunctionKind,
  FunctionOptions,
  ViewOptions,
  SchemaOptions,
} from 'spacetimedb';
export type { Infer } from 'spacetimedb';
