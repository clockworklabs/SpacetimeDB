import type { u128, u16, u256, u32 } from 'spacetime:sys@2.0';
import type { DatastoreBackend } from '../backend';
import type {
  TestRuntimeContext,
  TestRuntimeTarget,
  TestRuntimeTx,
} from './runtime';

class RowIteratorRegistry {
  #next = 1;
  #iters = new Map<number, Uint8Array[]>();

  add(rows: Uint8Array[]): number {
    const id = this.#next++;
    this.#iters.set(id, rows);
    return id;
  }

  advance(id: number, out: ArrayBuffer): number {
    const rows = this.#iters.get(id);
    if (!rows || rows.length === 0) {
      this.#iters.delete(id);
      return 0;
    }

    const required = rows.reduce((sum, row) => sum + row.byteLength, 0);
    if (required > out.byteLength) {
      const err = new Error('iterator output buffer too small') as Error & {
        __buffer_too_small__?: number;
      };
      err.__buffer_too_small__ = required;
      throw err;
    }

    const dst = new Uint8Array(out);
    let offset = 0;
    for (const row of rows) {
      dst.set(row, offset);
      offset += row.byteLength;
    }
    this.#iters.delete(id);
    return offset;
  }

  close(id: number) {
    this.#iters.delete(id);
  }
}

export class TestDatastoreBackend implements DatastoreBackend {
  readonly #ctx: TestRuntimeContext;
  readonly #target: TestRuntimeTarget;
  readonly #moduleIdentity: bigint;
  readonly #jwtPayloads = new Map<bigint, string>();
  readonly #iters = new RowIteratorRegistry();

  constructor(
    ctx: TestRuntimeContext,
    target: TestRuntimeTarget,
    moduleIdentity: bigint
  ) {
    this.#ctx = ctx;
    this.#target = target;
    this.#moduleIdentity = moduleIdentity;
  }

  withTransaction(tx: TestRuntimeTx): TestDatastoreBackend {
    const next = new TestDatastoreBackend(this.#ctx, tx, this.#moduleIdentity);
    for (const [connectionId, payload] of this.#jwtPayloads) {
      next.#jwtPayloads.set(connectionId, payload);
    }
    return next;
  }

  setJwtPayload(connectionId: bigint, payload: string) {
    this.#jwtPayloads.set(connectionId, payload);
  }

  identity(): u256 {
    return this.#moduleIdentity as u256;
  }

  getJwtPayload(connectionId: u128): Uint8Array {
    const payload = this.#jwtPayloads.get(connectionId);
    return payload ? new TextEncoder().encode(payload) : new Uint8Array();
  }

  tableIdFromName(name: string): u32 {
    return this.#ctx.tableId(name) as u32;
  }

  indexIdFromName(name: string): u32 {
    return this.#ctx.indexId(name) as u32;
  }

  datastoreTableRowCount(tableId: u32): number {
    return this.#ctx.tableRowCount(this.#target, tableId);
  }

  datastoreTableScanBsatn(tableId: u32): u32 {
    return this.#iters.add(this.#ctx.tableRows(this.#target, tableId)) as u32;
  }

  datastoreInsertBsatn(tableId: u32, row: ArrayBuffer, rowLen: number) {
    return this.#ctx.insertBsatn(
      this.#target,
      tableId,
      new Uint8Array(row, 0, rowLen)
    );
  }

  datastoreDeleteAllByEqBsatn(
    tableId: u32,
    row: ArrayBuffer,
    rowLen: number
  ): u32 {
    return this.#ctx.deleteAllByEqBsatn(
      this.#target,
      tableId,
      new Uint8Array(row, 0, rowLen)
    ) as u32;
  }

  datastoreIndexScanPointBsatn(
    indexId: u32,
    point: ArrayBuffer,
    pointLen: number
  ): u32 {
    return this.#iters.add(
      this.#ctx.indexScanPointBsatn(
        this.#target,
        indexId,
        new Uint8Array(point, 0, pointLen)
      )
    ) as u32;
  }

  datastoreIndexScanRangeBsatn(
    indexId: u32,
    prefix: ArrayBuffer,
    prefixLen: u32,
    prefixElems: u16,
    rstartLen: u32,
    rendLen: u32
  ): u32 {
    return this.#iters.add(
      this.#ctx.indexScanRangeBsatn(
        this.#target,
        indexId,
        new Uint8Array(prefix, 0, prefixLen + rstartLen + rendLen),
        prefixElems,
        rstartLen,
        rendLen
      )
    ) as u32;
  }

  datastoreDeleteByIndexScanPointBsatn(
    indexId: u32,
    point: ArrayBuffer,
    pointLen: number
  ): u32 {
    return this.#ctx.deleteByIndexScanPointBsatn(
      this.#target,
      indexId,
      new Uint8Array(point, 0, pointLen)
    ) as u32;
  }

  datastoreDeleteByIndexScanRangeBsatn(
    indexId: u32,
    prefix: ArrayBuffer,
    prefixLen: u32,
    prefixElems: u16,
    rstartLen: u32,
    rendLen: u32
  ): u32 {
    return this.#ctx.deleteByIndexScanRangeBsatn(
      this.#target,
      indexId,
      new Uint8Array(prefix, 0, prefixLen + rstartLen + rendLen),
      prefixElems,
      rstartLen,
      rendLen
    ) as u32;
  }

  datastoreUpdateBsatn(
    tableId: u32,
    indexId: u32,
    row: ArrayBuffer,
    rowLen: number
  ) {
    return this.#ctx.updateBsatn(
      this.#target,
      tableId,
      indexId,
      new Uint8Array(row, 0, rowLen)
    );
  }

  datastoreClear(tableId: u32): void {
    this.#ctx.clearTable(this.#target, tableId);
  }

  rowIterBsatnAdvance(iterId: u32, out: ArrayBuffer): number {
    return this.#iters.advance(iterId, out);
  }

  rowIterBsatnClose(iterId: u32): void {
    this.#iters.close(iterId);
  }

  procedureStartMutTx(): bigint {
    return 0n;
  }

  procedureCommitMutTx(): void {
    throw new Error('procedureCommitMutTx is handled by ProcedureTestBackend');
  }

  procedureAbortMutTx(): void {
    throw new Error('procedureAbortMutTx is handled by ProcedureTestBackend');
  }

  procedureHttpRequest(): [Uint8Array, Uint8Array] {
    throw new Error('test procedure HTTP requests are handled by TestHttpClient');
  }
}
