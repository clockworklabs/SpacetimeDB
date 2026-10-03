import { describe, expect, it } from 'vitest';
import {
  remoteModuleDeclFromExports,
  procedureSchema,
  procedures,
  reducerSchema,
  reducers,
  schema,
  t,
  table,
} from '../src/sdk';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { schema as moduleSchema, type Schema } from '../src/server/schema';

// A table declaration holds a `buildRawDef` closure, so two declarations of the
// same table are equal when their closures have the same source.
expect.addEqualityTesters([
  (a, b) =>
    typeof a === 'function' && typeof b === 'function'
      ? a.toString() === b.toString()
      : undefined,
]);

const person = table(
  {
    name: 'person',
    public: true,
    indexes: [{ accessor: 'age', algorithm: 'btree', columns: ['age'] }],
  },
  { id: t.u32().primaryKey().autoInc(), name: t.string(), age: t.u8() }
);

describe('remoteModuleDeclFromExports', () => {
  it('builds the remote module declaration that the old generated bindings build', () => {
    // The module as client bindings declare it, without bodies.
    const spacetimedb = schema({ person });
    const module = {
      default: spacetimedb,
      addPerson: spacetimedb.reducer(
        { name: 'add_person' },
        { name: t.string(), age: t.u8() }
      ),
      sayHello: spacetimedb.reducer({}),
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

    // The same module, the way the old generated bindings declare it. The old
    // client `schema()` built its tables with `tablesToSchema` directly.
    const tables = tablesToSchema(new ModuleContext(), {
      person,
      adults: table(
        { name: 'adults', indexes: [], constraints: [] },
        person.rowType
      ),
    }).tables;
    const reducersSchema = reducers(
      reducerSchema('add_person', { name: t.string(), age: t.u8() }),
      reducerSchema('say_hello', {})
    );
    const proceduresSchema = procedures(
      procedureSchema('person_count', { minAge: t.u8() }, t.u32())
    );

    const remoteModuleDecl = remoteModuleDeclFromExports(module);
    expect(remoteModuleDecl.tables).toEqual(tables);
    expect(remoteModuleDecl.reducers).toEqual(
      reducersSchema.reducersType.reducers
    );
    expect(remoteModuleDecl.procedures).toEqual(proceduresSchema.procedures);

    // The same module as its source, with bodies and a lifecycle reducer,
    // which a client leaves out.
    const source = moduleSchema({ person });
    expect(
      remoteModuleDeclFromExports({
        default: source,
        init: source.init(() => {}),
        addPerson: source.reducer(
          { name: 'add_person' },
          { name: t.string(), age: t.u8() },
          () => {}
        ),
        sayHello: source.reducer(() => {}),
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
      })
    ).toEqual(remoteModuleDecl);
  });

  it('rejects a declaration without a body outside client bindings', () => {
    const spacetimedb = moduleSchema({ person });
    // @ts-expect-error A module's reducer needs a body.
    const addPerson = spacetimedb.reducer({ name: t.string(), age: t.u8() });
    expect(() => spacetimedb.buildRawModuleDefV10({ addPerson })).toThrow(
      "The reducer 'addPerson' has no function body."
    );
  });

  it('rejects a declaration without a body for the host after a client registered it', () => {
    const spacetimedb = schema({ person });
    const module = {
      default: spacetimedb,
      addPerson: spacetimedb.reducer({ name: t.string(), age: t.u8() }),
    };
    remoteModuleDeclFromExports(module);
    const host = spacetimedb as unknown as Schema<any>;
    expect(() => host.buildRawModuleDefV10(module)).toThrow(
      "The reducer 'addPerson' has no function body."
    );
  });
});
