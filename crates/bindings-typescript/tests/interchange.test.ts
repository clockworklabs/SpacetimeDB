// Proposal 0040: a client can use a module's source in place of its generated
// bindings. These tests import the module in ../interchange-test-client, and
// its generated bindings, as a client build would: through the package's
// exports, with the `spacetimedb-client` condition (see vitest.config.ts), so
// they need `pnpm build` first.
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';
import * as client from 'spacetimedb';
import * as server from 'spacetimedb/server';
import * as source from '../interchange-test-client/spacetimedb/src/index';
import * as bindings from '../interchange-test-client/src/module_bindings/module';
import * as noCaseConversionSource from '../interchange-test-client/spacetimedb-no-case-conversion/src/index';
import * as noCaseConversionBindings from '../interchange-test-client/src/no_case_conversion_bindings/module';
import type { UntypedModuleDef } from '../src/sdk/spacetime_module';

describe('module source in a client build', () => {
  it('shares the client classes and has every value of spacetimedb/server', () => {
    // Every value that both entry points export must be the same object, so
    // that the client build bundles no second copy of the SDK's classes.
    const shared = Object.keys(client).filter(name => name in server);
    expect(shared).toEqual(expect.arrayContaining(['schema', 't']));
    for (const name of shared) {
      expect((server as Record<string, unknown>)[name], name).toBe(
        (client as Record<string, unknown>)[name]
      );
    }

    // The host build's value exports, read from its declarations so that
    // the host code does not run here.
    const declarations = fileURLToPath(
      new URL('../dist/server/index.d.ts', import.meta.url)
    );
    const program = ts.createProgram([declarations], { skipLibCheck: true });
    const checker = program.getTypeChecker();
    const values = checker
      .getExportsOfModule(
        checker.getSymbolAtLocation(program.getSourceFile(declarations)!)!
      )
      .filter(symbol => {
        const target =
          symbol.flags & ts.SymbolFlags.Alias
            ? checker.getAliasedSymbol(symbol)
            : symbol;
        return target.flags & ts.SymbolFlags.Value;
      })
      .map(symbol => symbol.name);
    expect(values).toContain('schema');
    expect(Object.keys(server)).toEqual(expect.arrayContaining(values));
  });

  it('gives the client the module def that the generated bindings give it', () => {
    expect(clientView(client.moduleDefFromExports(source))).toEqual(
      clientView(client.moduleDefFromExports(bindings))
    );
  });

  it('gives the client the names that the host gives under another case conversion policy', () => {
    expect(
      clientView(client.moduleDefFromExports(noCaseConversionSource))
    ).toEqual(
      clientView(client.moduleDefFromExports(noCaseConversionBindings))
    );
  });
});

/**
 * What a client, or the host, can observe of a module def, as plain data.
 * Generated bindings declare a primary key or unique column both on the column
 * and in the table's lists of indexes and constraints, so those lists can
 * repeat an entry, and they are compared as sets. Bindings also order their
 * functions by name, and the source by export, which a client does not rely on.
 */
function clientView(moduleDef: UntypedModuleDef) {
  const set = <T>(items: readonly T[]) =>
    [...new Set(items.map(item => JSON.stringify(item, bigints)))].sort();
  return {
    tables: Object.fromEntries(
      Object.entries(moduleDef.tables).map(([accessorName, table]) => [
        accessorName,
        {
          name: table.sourceName,
          isEvent: table.isEvent ?? false,
          access: table.rawDef.tableAccess.tag,
          columns: Object.entries(table.columns).map(([key, column]) => ({
            accessorName: key,
            name: column.columnMetadata.name ?? key,
            type: column.typeBuilder.algebraicType,
          })),
          primaryKey: table.rawDef.primaryKey,
          autoInc: set(table.rawDef.sequences.map(seq => seq.column)),
          defaults: table.rawDef.defaultValues,
          indexes: set(table.resolvedIndexes),
          uniqueColumns: set(table.constraints.map(c => c.columns)),
        },
      ])
    ),
    reducers: byAccessor(moduleDef.reducers).map(reducer => ({
      name: reducer.name,
      accessorName: reducer.accessorName,
      params: reducer.paramsType,
    })),
    procedures: byAccessor(moduleDef.procedures).map(procedure => ({
      name: procedure.name,
      accessorName: procedure.accessorName,
      params: Object.entries(procedure.params).map(([key, param]) => [
        key,
        param.algebraicType,
      ]),
      returnType: procedure.returnType.algebraicType,
    })),
  };
}

function byAccessor<T extends { accessorName: string }>(items: readonly T[]) {
  return [...items].sort((a, b) =>
    a.accessorName.localeCompare(b.accessorName)
  );
}

function bigints(_key: string, value: unknown) {
  return typeof value === 'bigint' ? value.toString() : value;
}
