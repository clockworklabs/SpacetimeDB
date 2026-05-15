import { describe, expect, test } from 'vitest';
import { ConnectionId, Identity, TimeDuration, Timestamp, Uuid } from '../src';

// The constructors of these wrapper classes claim to take `bigint` (or
// `string | bigint` for Identity), but real-world callers can end up
// passing `number` — typically after data goes through JSON in some
// external layer (HTTP responses, state caches, custom serializers),
// since JSON has no `bigint` and `JSON.parse` produces `number`. Before
// the coercion patch this silently corrupted the field and crashed
// later during BSATN serialization with an opaque mix-error. These
// tests pin the new behavior: numeric inputs are coerced to bigint and
// the field is always a bigint.

describe('id-like constructors coerce non-bigint numerics', () => {
  test('Identity(number) stores a bigint', () => {
    const id = new Identity(123 as unknown as bigint);
    expect(typeof id.__identity__).toBe('bigint');
    expect(id.__identity__).toBe(123n);
  });

  test('Identity(bigint) stays bigint', () => {
    const id = new Identity(123n);
    expect(typeof id.__identity__).toBe('bigint');
    expect(id.__identity__).toBe(123n);
  });

  test('Identity(hex string) still parses', () => {
    const id = new Identity(
      '0x0000000000000000000000000000000000000000000000000000000000000001'
    );
    expect(typeof id.__identity__).toBe('bigint');
    expect(id.__identity__).toBe(1n);
  });

  test('ConnectionId(number) stores a bigint', () => {
    const c = new ConnectionId(42 as unknown as bigint);
    expect(typeof c.__connection_id__).toBe('bigint');
    expect(c.__connection_id__).toBe(42n);
    // Downstream consumers rely on bigint operators working:
    expect(c.isZero()).toBe(false);
    expect(c.toHexString().length).toBeGreaterThan(0);
  });

  test('Timestamp(number) stores a bigint', () => {
    const t = new Timestamp(1_000_000 as unknown as bigint);
    expect(typeof t.__timestamp_micros_since_unix_epoch__).toBe('bigint');
    expect(t.microsSinceUnixEpoch).toBe(1_000_000n);
    // toDate() does bigint arithmetic — would throw mix-error before.
    expect(t.toDate().getTime()).toBe(1000);
  });

  test('TimeDuration(number) stores a bigint', () => {
    const d = new TimeDuration(5_000_000 as unknown as bigint);
    expect(typeof d.__time_duration_micros__).toBe('bigint');
    expect(d.micros).toBe(5_000_000n);
  });

  test('Uuid(number) is coerced and range-checked', () => {
    const u = new Uuid(0 as unknown as bigint);
    expect(typeof u.__uuid__).toBe('bigint');
    expect(u.__uuid__).toBe(0n);
  });

  test('Uuid(out-of-range bigint) still throws', () => {
    expect(() => new Uuid(Uuid.MAX_UUID_BIGINT + 1n)).toThrow(/Invalid UUID/);
    expect(() => new Uuid(-1n)).toThrow(/Invalid UUID/);
  });

  test('Identity(undefined) throws a clear error instead of corrupting', () => {
    expect(() => new Identity(undefined as unknown as bigint)).toThrow();
  });
});
