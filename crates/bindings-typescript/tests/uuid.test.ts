import { describe, expect, test } from 'vitest';
import { Timestamp, Uuid } from '../src';
import * as crypto from 'crypto';

describe('Uuid', () => {
  test('toString UUid', () => {
    const uuids = [
      Uuid.NIL,
      new Uuid(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10n),
      Uuid.MAX,
    ];

    for (const uuid of uuids) {
      const s = uuid.toString();
      const uuid2 = Uuid.parse(s);
      expect(s).toBe(uuid2.toString());
      // Bigint structural equality
      expect(uuid.asBigInt()).toBe(uuid2.asBigInt());
    }
  });

  test('round_trip', () => {
    const u1 = Uuid.NIL;
    const s = u1.toString();
    const u2 = Uuid.parse(s);

    expect(u1.toString()).toBe(u2.toString());
    expect(u1).toStrictEqual(u2);
    expect(s).toBe(u2.toString());
  });

  test('version', () => {
    let u = Uuid.NIL;
    expect(u.getVersion()).toBe('Nil');
    u = Uuid.MAX;
    expect(u.getVersion()).toBe('Max');

    const randomBytes = crypto.getRandomValues(new Uint8Array(16));
    u = Uuid.fromRandomBytesV4(randomBytes);
    expect(u.getVersion()).toBe('V4');

    const counter = { value: Number(0) };
    u = Uuid.fromCounterV7(
      counter,
      new Timestamp(1_686_000_000_000n),
      randomBytes.slice(0, 4)
    );
    expect(u.getVersion()).toBe('V7');
  });
  test('wrap_around', () => {
    // Check wraparound behavior
    const counter = { value: 0x7fffffff }; // i32::MAX

    Uuid.fromCounterV7(counter, Timestamp.now(), new Uint8Array(4));

    expect(counter.value).toBe(0);
  });
  test('negative_timestamp_error', () => {
    const counter = { value: 0 };
    const ts = new Timestamp(-1n);

    expect(() => {
      Uuid.fromCounterV7(counter, ts, new Uint8Array(4));
    }).toThrow('`fromCounterV7` `timestamp` before unix epoch');
  });
  test('ordered', () => {
    // from_u128 equivalent:
    const u1 = new Uuid(1n);
    const u2 = new Uuid(2n);

    expect(u1.compareTo(u2)).toBeLessThan(0);
    expect(u2.compareTo(u1)).toBeGreaterThan(0);
    expect(u1.compareTo(u1)).toBe(0);
    expect(u1.compareTo(u2)).not.toBe(0);

    // Check we start from zero
    const counterStart = { value: 0 };
    const tsStart = Timestamp.now();

    const uStart = Uuid.fromCounterV7(counterStart, tsStart, new Uint8Array(4));

    expect(uStart.getCounter()).toBe(0);

    // Check ordering over many UUIDs up to the max counter value
    const total = 10_000;
    const counter = { value: 0x7fffffff - total };
    const ts = Timestamp.now();

    const bytes = crypto.getRandomValues(new Uint8Array(4));
    let a = Uuid.fromCounterV7(counter, ts, bytes);

    for (let i = 0; i < total; i++) {
      const b = Uuid.fromCounterV7(counter, ts, bytes);

      expect(a.getVersion()).toBe('V7');

      expect(
        a.compareTo(b),
        `UUIDs are not ordered at ${i}: ${a.toString()} !< ${b.toString()}`
      ).toBeLessThan(0);

      expect(
        a.getCounter(),
        `UUID counters are not ordered at ${i}: ${a.getCounter()} !< ${b.getCounter()}`
      ).toBeLessThan(b.getCounter());
      a = b;
    }
  });
});
