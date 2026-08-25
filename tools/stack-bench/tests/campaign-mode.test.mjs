import assert from 'node:assert/strict';
import test from 'node:test';

import { CAMPAIGN_MODE_REGISTRY, createCampaignModeRegistry }
  from '../src/campaigns/campaign-mode.mjs';

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
    id: 'dependency', version: '1.0.0',
  }), { id: 'dependency', version: '1.0.0' });
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
