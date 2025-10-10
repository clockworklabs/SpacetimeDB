import { describe, expect, test } from 'vitest';
import { ClockGenerator, Timestamp, Uuid } from '../src';
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

    const clock = new ClockGenerator(new Timestamp(BigInt(Date.now()) * 1000n));
    const first10 = randomBytes.slice(0, 10);
    u = Uuid.fromClockV7(clock, first10);
    expect(u.getVersion()).toBe('V7');
  });

  test('ordered', () => {
    // from_u128 equivalent:
    const u1 = new Uuid(1n);
    const u2 = new Uuid(2n);

    expect(u1.compareTo(u2)).toBeLessThan(0);
    expect(u2.compareTo(u1)).toBeGreaterThan(0);
    expect(u1.compareTo(u1)).toBe(0);
    expect(u1.compareTo(u2)).not.toBe(0);

    const clock = new ClockGenerator(new Timestamp(BigInt(Date.now()) * 1000n));

    const uuids: Uuid[] = Array.from({ length: 1000 }, () => {
      const arr = new Uint8Array(10);
      crypto.getRandomValues(arr);
      return Uuid.fromClockV7(clock, arr);
    });

    // validate monotonic ordering
    for (let i = 0; i < uuids.length - 1; i++) {
      const a = uuids[i];
      const b = uuids[i + 1];

      if (!(a.compareTo(b) < 0)) {
        throw new Error(`UUIDs are not ordered at ${i}: ${a} !< ${b}`);
      }
    }
  });
});
