import { afterEach, describe, expect, it } from 'vitest';

import { schema, table, t } from '../src/server';
import { createModuleTestHarness, TestAuth } from '../src/server/test-utils';
import { createWasmTestRuntime } from '../src/server/test-utils/wasm';

type CommitMode = 'Normal' | 'DropEventTableRows';

class FakeTx {
  readonly __nativeTxBrand!: unique symbol;
  readonly __wasmTxBrand!: unique symbol;

  constructor(public rows: Uint8Array[]) {}
}

class FakeWasmPortableDatastore {
  static instances: FakeWasmPortableDatastore[] = [];

  rows: Uint8Array[] = [];
  commitModes: CommitMode[] = [];

  constructor(
    _rawModuleDef: Uint8Array,
    readonly moduleIdentityHex: string
  ) {
    FakeWasmPortableDatastore.instances.push(this);
  }

  tableId(name: string): number {
    if (name !== 'person') throw new Error(`unknown table: ${name}`);
    return 1;
  }

  indexId(name: string): number {
    if (name !== 'person_id_idx_btree')
      throw new Error(`unknown index: ${name}`);
    return 1;
  }

  beginMutTx(): FakeTx {
    return new FakeTx(this.rows.map(row => row.slice()));
  }

  commitTx(tx: FakeTx, mode: CommitMode): void {
    this.rows = tx.rows.map(row => row.slice());
    this.commitModes.push(mode);
  }

  rollbackTx(_tx: FakeTx): void {}

  reset(): void {
    this.rows = [];
    this.commitModes = [];
  }

  tableRowCount(_tableId: number): number {
    return this.rows.length;
  }

  tableRowCountTx(tx: FakeTx, _tableId: number): number {
    return tx.rows.length;
  }

  tableRowsBsatn(_tableId: number): Uint8Array[] {
    return this.rows.map(row => row.slice());
  }

  tableRowsBsatnTx(tx: FakeTx, _tableId: number): Uint8Array[] {
    return tx.rows.map(row => row.slice());
  }

  indexScanPointBsatn(): Uint8Array[] {
    return [];
  }

  indexScanPointBsatnTx(): Uint8Array[] {
    return [];
  }

  indexScanRangeBsatn(): Uint8Array[] {
    return [];
  }

  indexScanRangeBsatnTx(): Uint8Array[] {
    return [];
  }

  insertBsatnGeneratedCols(
    tx: FakeTx,
    _tableId: number,
    row: Uint8Array
  ): Uint8Array {
    tx.rows.push(row.slice());
    return new Uint8Array();
  }

  updateBsatnGeneratedCols(
    tx: FakeTx,
    _tableId: number,
    _indexId: number,
    row: Uint8Array
  ): Uint8Array {
    tx.rows[0] = row.slice();
    return new Uint8Array();
  }

  deleteByRelBsatn(tx: FakeTx, _tableId: number, relation: Uint8Array): number {
    const row = relation.subarray(4);
    const before = tx.rows.length;
    tx.rows = tx.rows.filter(existing => !bytesEqual(existing, row));
    return before - tx.rows.length;
  }

  deleteByIndexScanPointBsatn(): number {
    return 0;
  }

  deleteByIndexScanRangeBsatn(): number {
    return 0;
  }

  clearTable(tx: FakeTx, _tableId: number): number {
    const count = tx.rows.length;
    tx.rows = [];
    return count;
  }

  validateJwtPayload(): { senderHex: string; connectionIdHex: undefined } {
    return { senderHex: '00'.repeat(32), connectionIdHex: undefined };
  }

  runQuery(): Uint8Array[] {
    throw new Error('fake wasm adapter test does not implement runQuery');
  }
}

const fakeWasmModule = { WasmPortableDatastore: FakeWasmPortableDatastore };

afterEach(() => {
  globalThis.__spacetimedbTestRuntime = undefined;
  FakeWasmPortableDatastore.instances = [];
});

describe('server test-utils wasm runtime adapter', () => {
  it('supports direct test.db writes through auto-commit transactions', () => {
    const { spacetime, moduleExports } = makeModule();
    installFakeWasmRuntime();

    const test = createModuleTestHarness(spacetime, moduleExports);

    test.db.person.insert({ id: 1, name: 'Alice' });

    expect(test.db.person.count()).toBe(1n);
    expect([...test.db.person.iter()]).toEqual([{ id: 1, name: 'Alice' }]);
    expect(FakeWasmPortableDatastore.instances[0].commitModes).toEqual([
      'Normal',
    ]);
  });

  it('commits reducer transactions with event-table cleanup mode', () => {
    const { spacetime, moduleExports, addPerson } = makeModule();
    installFakeWasmRuntime();

    const test = createModuleTestHarness(spacetime, moduleExports);

    test.withReducerTx(TestAuth.internal(), ctx => {
      addPerson(ctx, { id: 2, name: 'Bob' });
    });

    expect([...test.db.person.iter()]).toEqual([{ id: 2, name: 'Bob' }]);
    expect(FakeWasmPortableDatastore.instances[0].commitModes).toEqual([
      'DropEventTableRows',
    ]);
  });

  it('rolls back reducer transactions when the body throws', () => {
    const { spacetime, moduleExports, addPerson } = makeModule();
    installFakeWasmRuntime();

    const test = createModuleTestHarness(spacetime, moduleExports);

    expect(() =>
      test.withReducerTx(TestAuth.internal(), ctx => {
        addPerson(ctx, { id: 3, name: 'Carol' });
        throw new Error('fail');
      })
    ).toThrow('fail');

    expect(test.db.person.count()).toBe(0n);
    expect(FakeWasmPortableDatastore.instances[0].commitModes).toEqual([]);
  });
});

function makeModule() {
  const person = table(
    { name: 'person', public: true },
    {
      id: t.u32(),
      name: t.string(),
    }
  );
  const spacetime = schema({ person });
  const addPerson = spacetime.reducer(
    { id: t.u32(), name: t.string() },
    (ctx, row) => {
      ctx.db.person.insert(row);
    }
  );

  return { spacetime, moduleExports: { addPerson }, addPerson };
}

function installFakeWasmRuntime() {
  globalThis.__spacetimedbTestRuntime = createWasmTestRuntime(fakeWasmModule);
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  return a.byteLength === b.byteLength && a.every((value, i) => value === b[i]);
}
