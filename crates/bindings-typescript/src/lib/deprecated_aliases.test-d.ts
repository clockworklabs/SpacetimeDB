import type {
  ModuleDef,
  RemoteModule,
  TableDefByName,
  TableDefForTableName,
  TableDefOf,
  TableNames,
  TableNamesOf,
} from '..';
import type { UntypedProceduresDef } from '../sdk/procedures';
import type { UntypedReducersDef } from '../sdk/reducers';
import type { UntypedSchemaDef } from './schema';

type Equals<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
type Assert<T extends true> = T;

// Each deprecated alias must stay identical to the type that replaced it.
type _RemoteModule = Assert<
  Equals<
    RemoteModule<UntypedSchemaDef, UntypedReducersDef, UntypedProceduresDef>,
    ModuleDef<UntypedSchemaDef, UntypedReducersDef, UntypedProceduresDef>
  >
>;
type _TableNames = Assert<
  Equals<TableNames<UntypedSchemaDef>, TableNamesOf<UntypedSchemaDef>>
>;
type _TableDefByName = Assert<
  Equals<
    TableDefByName<UntypedSchemaDef, string>,
    TableDefOf<UntypedSchemaDef, string>
  >
>;
type _TableDefForTableName = Assert<
  Equals<
    TableDefForTableName<UntypedSchemaDef, string>,
    TableDefOf<UntypedSchemaDef, string>
  >
>;
