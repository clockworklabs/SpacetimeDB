import * as assert from 'node:assert/strict';
import {
  COOLANT_UNLOCK_LEVEL,
  SURGE_UNLOCK_LEVEL,
  hasCoolantFlush,
  hasSurgeBurst,
  roomTuning,
  tapLimitForState,
  upgradeOffer,
  upgradeWindowForState,
  type UpgradeState,
} from '../spacetimedb/src/reactor-rules';

const state: UpgradeState = {
  powerUpgradeCount: 0,
  coolingUpgradeCount: 0,
  capacityUpgradeCount: 0,
  chargeUpgradeCount: 0,
  bayUpgradeCount: 0,
};

assert.deepEqual(roomTuning(state, 1), {
  heatCapacity: 100,
  coolingPerSecond: 4,
  tapHeatGain: 12,
});
assert.deepEqual(roomTuning(state, 3), {
  heatCapacity: 250,
  coolingPerSecond: 12,
  tapHeatGain: 12,
});
assert.equal(upgradeOffer(state, 'power').cost, 12n);
assert.equal(tapLimitForState({ chargeUpgradeCount: 2 }), 12);
assert.equal(upgradeWindowForState({ bayUpgradeCount: 99 }), 5);
assert.equal(
  hasCoolantFlush({ coolingUpgradeCount: COOLANT_UNLOCK_LEVEL }),
  true
);
assert.equal(
  hasSurgeBurst({ powerUpgradeCount: SURGE_UNLOCK_LEVEL - 1 }),
  false
);

console.log('reactor rules tests passed');
