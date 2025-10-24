declare module 'spacetime:sys@1.0' {
  export type u8 = number;
  export type u16 = number;
  export type u32 = number;
  export type u64 = bigint;
  export type u128 = bigint;
  export type u256 = bigint;

  export type ModuleHooks = {
    __describe_module__(): Uint8Array;

    __call_reducer__(
      reducerId: u32,
      sender: u256,
      connId: u128,
      timestamp: bigint,
      argsBuf: Uint8Array
    ): { tag: 'ok' } | { tag: 'err'; value: string };
  };

  export function register_hooks(hooks: ModuleHooks);

  export function table_id_from_name(name: string): u32;
  export function index_id_from_name(name: string): u32;
  export function datastore_table_row_count(table_id: u32): u64;
  export function datastore_table_scan_bsatn(table_id: u32): u32;
  export function datastore_index_scan_range_bsatn(
    index_id: u32,
    prefix: Uint8Array,
    prefix_elems: u16,
    rstart: Uint8Array,
    rend: Uint8Array
  ): u32;
  export function row_iter_bsatn_advance(
    iter: u32,
    buffer_max_len: u32
  ): [boolean, Uint8Array];
  export function row_iter_bsatn_close(iter: u32): void;
  export function datastore_insert_bsatn(
    table_id: u32,
    row: Uint8Array
  ): Uint8Array;
  export function datastore_update_bsatn(
    table_id: u32,
    index_id: u32,
    row: Uint8Array
  ): Uint8Array;
  export function datastore_delete_by_index_scan_range_bsatn(
    index_id: u32,
    prefix: Uint8Array,
    prefix_elems: u16,
    rstart: Uint8Array,
    rend: Uint8Array
  ): u32;
  export function datastore_delete_all_by_eq_bsatn(
    table_id: u32,
    relation: Uint8Array
  ): u32;
  export function volatile_nonatomic_schedule_immediate(
    reducer_name: string,
    args: Uint8Array
  ): void;
  export function console_log(level: u8, message: string): void;
  export function console_timer_start(name: string): u32;
  export function console_timer_end(span_id: u32): void;
  export function identity(): { __identity__: u256 };
  export function get_jwt_payload(connection_id: u128): Uint8Array;
}
