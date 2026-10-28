/** Client module declarations, sharing builder identities with the root SDK. */
export {
  declarationSchema as schema,
  DeclarationSchema as Schema,
  FunctionDeclaration,
  t,
  table,
  CaseConversionPolicy,
} from 'spacetimedb';
export type {
  ModuleDeclarations,
  SchemaEntry,
  SchemaEntries,
  DeclarationKind,
  FunctionOptions,
  ViewOptions,
  SchemaOptions,
} from 'spacetimedb';
export type { Infer } from 'spacetimedb';
