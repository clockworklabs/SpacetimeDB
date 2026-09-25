/**
 * Test stub for the host-provided `spacetime:sys@2.0` / `spacetime:sys@2.1`
 * virtual modules.
 *
 * In a real module these syscalls are injected by the SpacetimeDB V8 host. For
 * unit tests we only need enough behaviour to drive the pure-JS portions of
 * `runtime.ts` (e.g. index `serializeRange`). Every iterator-producing call
 * returns a dummy iterator id whose `advance` immediately reports "empty", so
 * `[...filter(...)]` deserializes zero rows without touching a real datastore.
 */

export const moduleHooks: unique symbol = Symbol('moduleHooks') as any;

export const table_id_from_name = (_name: string): number => 1;
export const index_id_from_name = (_name: string): number => 1;

export const datastore_table_row_count = (_table_id: number): bigint => 0n;
export const datastore_table_scan_bsatn = (_table_id: number): number => 1;

export const datastore_index_scan_range_bsatn = (
  _index_id: number,
  _buf: ArrayBuffer,
  _prefix_len: number,
  _prefix_elems: number,
  _rstart_len: number,
  _rend_len: number
): number => 1;

export const datastore_index_scan_point_bsatn = (
  _index_id: number,
  _point: ArrayBuffer,
  _point_len: number
): number => 1;

export const datastore_delete_by_index_scan_range_bsatn = (
  _index_id: number,
  _buf: ArrayBuffer,
  _prefix_len: number,
  _prefix_elems: number,
  _rstart_len: number,
  _rend_len: number
): number => 0;

export const datastore_delete_by_index_scan_point_bsatn = (
  _index_id: number,
  _point: ArrayBuffer,
  _point_len: number
): number => 0;

// `0` => iterator is empty and has been destroyed (see advanceIterRaw docs).
export const row_iter_bsatn_advance = (
  _iter: number,
  _buffer: ArrayBuffer
): number => 0;
export const row_iter_bsatn_close = (_iter: number): void => {};

export const datastore_insert_bsatn = (
  _table_id: number,
  _row: ArrayBuffer,
  _row_len: number
): number => 0;
export const datastore_update_bsatn = (
  _table_id: number,
  _index_id: number,
  _row: ArrayBuffer,
  _row_len: number
): number => 0;
export const datastore_delete_all_by_eq_bsatn = (
  _table_id: number,
  _relation: ArrayBuffer,
  _relation_len: number
): number => 0;
export const datastore_clear = (_table_id: number): bigint => 0n;

export const volatile_nonatomic_schedule_immediate = (
  _reducer_name: string,
  _args: Uint8Array
): void => {};
export const console_log = (_level: number, _message: string): void => {};
export const console_timer_start = (_name: string): number => 0;
export const console_timer_end = (_span_id: number): void => {};
export const identity = (): bigint => 0n;
export const get_jwt_payload = (_connection_id: bigint): Uint8Array =>
  new Uint8Array();
export const register_hooks = (_hooks: unknown): void => {};

export const procedure_http_request = (
  _request: Uint8Array,
  _body: Uint8Array | string
): [Uint8Array, Uint8Array] => [new Uint8Array(), new Uint8Array()];
export const procedure_start_mut_tx = (): bigint => 0n;
export const procedure_commit_mut_tx = (): void => {};
export const procedure_abort_mut_tx = (): void => {};
