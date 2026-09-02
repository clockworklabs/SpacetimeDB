import assert from 'node:assert/strict';
import test from 'node:test';

import { CAMPAIGN_MODE_REGISTRY, createCampaignModeRegistry }
  from '../src/campaigns/campaign-mode.js';

test('the campaign mode registry requires an exact supported mode', () => {
  assert.deepEqual(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'sequential', version: '1.0.0',
  }), { id: 'sequential', version: '1.0.0' });
  assert.throws(() => CAMPAIGN_MODE_REGISTRY.validate({
    id: 'sequential', version: '2.0.0',
  }), /unknown sequential@2\.0\.0/);
  assert.throws(() => CAMPAIGN_MODE_REGISTRY.validate({
    id: 'sequential', version: '1.0.0', extra: true,
  }), /extra is unknown/);
  assert.deepEqual(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', version: '3.2.0', strikes: { default: 3, levels: {} },
  }), { id: 'dependency', version: '3.2.0', repairSelection: 'feature', strikePolicy: 'feature',
    workSelection: 'progressive',
    strikes: { default: 3, levels: {} } });
  assert.deepEqual(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', version: '3.2.0', repairSelection: 'batch',
    strikePolicy: 'banked', strikes: { default: 3, levels: {} },
  }), { id: 'dependency', version: '3.2.0', repairSelection: 'batch',
    strikePolicy: 'banked', workSelection: 'progressive',
    strikes: { default: 3, levels: {} } });
  assert.equal(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', version: '3.2.0', workSelection: 'all-at-once',
    strikes: { default: 1, levels: {} },
  }).workSelection, 'all-at-once');
  assert.throws(() => CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', version: '2.1.0', strikes: { default: 3, levels: {} },
  }), /unknown dependency@2\.1\.0/);
});

test('new modes can be registered without changing campaign validation', () => {
  const registry = createCampaignModeRegistry([{
    id: 'example',
    version: '1.0.0',
    validate(value) {
      if (typeof value.definition !== 'string' || !value.definition) {
        throw new Error('example mode requires definition');
      }
      return value;
    },
  }]);
  assert.deepEqual(registry.validate({
    id: 'example', version: '1.0.0', definition: 'example@1.0.0',
  }), { id: 'example', version: '1.0.0', definition: 'example@1.0.0' });
});
