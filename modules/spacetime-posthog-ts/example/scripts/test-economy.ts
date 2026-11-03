import assert from 'node:assert/strict';

import {
  arrivalDemand,
  maximumQueueLength,
  seededRandom,
  serviceCapacity,
  storageCapacity,
  upgradeCost,
} from '../spacetimedb/src/economy';
import type { EconRow } from '../spacetimedb/src/schema';

const economy = {
  workers: 2,
  machineLevel: 1,
  storageLevel: 2,
  seats: 3,
} as EconRow;

assert.equal(storageCapacity('context', 0), 250);
assert.equal(storageCapacity('context', 2), 500);
assert.equal(serviceCapacity(economy), 2);
assert.equal(maximumQueueLength(economy), 12);
assert.equal(arrivalDemand(10, 50), 10);
assert.equal(upgradeCost('worker', economy), 12_000n);
assert.equal(upgradeCost('machine', economy), 16_000n);
assert.equal(upgradeCost('storage', economy), 21_000n);
assert.equal(upgradeCost('counter', economy), 20_000n);

const first = seededRandom('stable-seed');
const second = seededRandom('stable-seed');
for (let index = 0; index < 10; index++) {
  const value = first();
  assert.equal(value, second());
  assert.ok(value >= 0 && value < 1);
}

console.log('posthog economy tests passed');
