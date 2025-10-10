import { describe, expect, test } from 'vitest';
import { ClockGenerator, Timestamp, Uuid } from '../src';

function newUuidV4(): Uuid {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Uuid.fromRandomBytesV4(bytes);
}

function newUuidV7(clock: ClockGenerator): Uuid {
  const random = crypto.getRandomValues(new Uint8Array(10));
  return Uuid.fromClockV7(clock, random);
}

describe('Uuid', () => {
  test('toString UUid', () => {
    const uuid = Uuid.NIL;
    expect(uuid.toString()).toBe('00000000-0000-0000-0000-000000000000');
    const parsed = Uuid.parse('00000000-0000-0000-0000-000000000000');
    expect(parsed.asBigInt()).toBe(0n);
  });

  test('sorted uuid', () => {
    const clock = new ClockGenerator(new Timestamp(0n));
    const uuids: Uuid[] = [];
    for (let i = 0; i < 1000; i++) {
      const uuid = newUuidV7(clock);
      uuids.push(uuid);
    }
    const sortedUuids = [...uuids].sort((a, b) => {
      if (a.asBigInt() < b.asBigInt()) return -1;
      if (a.asBigInt() > b.asBigInt()) return 1;
      return 0;
    });
    expect(uuids).toEqual(sortedUuids);
  });
});
