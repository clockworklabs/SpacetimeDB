import { table } from '../lib/table';
import { t } from '../lib/type_builders';
import { procedureSchema, procedures } from './procedures';
import { reducerSchema, reducers } from './reducers';
import { schema, type InferRemoteModuleDecl } from './schema';
import { schema as moduleSchema } from '../server/schema';
import type { UntypedRemoteModuleDecl } from './spacetime_module';

type Equals<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
type Extends<A, B> = [A] extends [B] ? true : false;
type Assert<T extends true> = T;

const person = table(
  {
    name: 'person',
    public: true,
    indexes: [{ accessor: 'age', algorithm: 'btree', columns: ['age'] }],
  },
  { id: t.u32().primaryKey().autoInc(), name: t.string(), age: t.u8() }
);

// The module as client bindings declare it, without bodies.
const spacetimedb = schema({ person });
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const module = {
  default: spacetimedb,
  addPerson: spacetimedb.reducer(
    { name: 'add_person' },
    { name: t.string(), age: t.u8() }
  ),
  personCount: spacetimedb.procedure(
    { name: 'person_count' },
    { minAge: t.u8() },
    t.u32()
  ),
  adults: spacetimedb.view(
    { name: 'adults', public: true },
    t.array(person.rowType)
  ),
};
type Module = InferRemoteModuleDecl<typeof module>;

// The same module as its source, with bodies and a lifecycle reducer, which a
// client leaves out.
const source = moduleSchema({ person });
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const moduleSource = {
  default: source,
  init: source.init(() => {}),
  addPerson: source.reducer(
    { name: 'add_person' },
    { name: t.string(), age: t.u8() },
    () => {}
  ),
  personCount: source.procedure(
    { name: 'person_count' },
    { minAge: t.u8() },
    t.u32(),
    () => 0
  ),
  adults: source.view(
    { name: 'adults', public: true },
    t.array(person.rowType),
    () => []
  ),
};
type _Source = Assert<
  Equals<InferRemoteModuleDecl<typeof moduleSource>, Module>
>;

// A module's declarations need bodies, and client bindings' declarations have none.
// @ts-expect-error a module's reducer needs a body.
source.reducer({ name: t.string() });
// @ts-expect-error client bindings cannot declare a reducer with a body.
spacetimedb.reducer({ name: t.string() }, () => {});

// The same module, the way the old generated bindings declare it.
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const tablesSchema = schema({
  person,
  adults: table(
    { name: 'adults', indexes: [], constraints: [] },
    person.rowType
  ),
});
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const reducersSchema = reducers(
  reducerSchema('add_person', { name: t.string(), age: t.u8() })
);
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const proceduresSchema = procedures(
  procedureSchema('person_count', { minAge: t.u8() }, t.u32())
);

// `InferRemoteModuleDecl` gives the remote module declaration the type that
// the old bindings give it.
type _Tables = Assert<
  Equals<Module['tables'], typeof tablesSchema.schemaType.tables>
>;
type _Reducers = Assert<
  Equals<
    Module['reducers'][number],
    (typeof reducersSchema.reducersType.reducers)[number]
  >
>;
type _Procedures = Assert<
  Equals<
    Omit<Module['procedures'][number], 'name'>,
    Omit<(typeof proceduresSchema.procedures)[number], 'name'>
  >
>;
// The host converts a procedure's explicit name, so its name is a string.
type _ProcedureName = Assert<
  Equals<Module['procedures'][number]['name'], string>
>;
type _RemoteModuleDecl = Assert<Extends<Module, UntypedRemoteModuleDecl>>;
