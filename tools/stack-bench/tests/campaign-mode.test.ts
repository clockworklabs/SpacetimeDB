import assert from 'node:assert/strict';
import test from 'node:test';

import { CAMPAIGN_MODE_REGISTRY, createCampaignModeRegistry }
  from '../src/campaigns/campaign-mode.js';

test('the campaign mode registry requires an exact supported mode', () => {
  assert.deepEqual(CAMPAIGN_MODE_REGISTRY.validate({ id: 'sequential' }), { id: 'sequential' });
  assert.throws(() => CAMPAIGN_MODE_REGISTRY.validate({
    id: 'sequential', version: '1.0.0',
  }), /version is unknown/);
  assert.throws(() => CAMPAIGN_MODE_REGISTRY.validate({
    id: 'sequential', extra: true,
  }), /extra is unknown/);
  assert.deepEqual(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency',
  }), { id: 'dependency', workSelection: 'progressive' });
  assert.equal(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', workSelection: 'feature',
  }).workSelection, 'feature');
  assert.equal(CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', workSelection: 'all-at-once',
  }).workSelection, 'all-at-once');
  assert.throws(() => CAMPAIGN_MODE_REGISTRY.validate({
    id: 'dependency', repairSelection: 'feature',
  }), /repairSelection is unknown/);
});

test('new modes can be registered without changing campaign validation', () => {
  const registry = createCampaignModeRegistry([{
    id: 'example',
    validate(value) {
      if (typeof value.definition !== 'string' || !value.definition) {
        throw new Error('example mode requires definition');
      }
      return value;
    },
  }]);
  assert.deepEqual(registry.validate({
    id: 'example', definition: 'example',
  }), { id: 'example', definition: 'example' });
});
