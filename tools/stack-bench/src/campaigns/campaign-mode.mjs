export const CAMPAIGN_MODE_SCHEMA_VERSION = 1;

const ID = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function fail(message) {
  throw new Error(`invalid campaign mode: ${message}`);
}

function validateIdentity(value, at) {
  if (!object(value)) fail(`${at} must be an object`);
  if (typeof value.id !== 'string' || !ID.test(value.id)) fail(`${at}.id is invalid`);
  if (typeof value.version !== 'string' || !VERSION.test(value.version)) {
    fail(`${at}.version must be an exact semantic version`);
  }
}

export function createCampaignModeRegistry(modes) {
  if (!Array.isArray(modes) || modes.length === 0) fail('registry requires at least one mode');
  const entries = new Map();
  for (const mode of modes) {
    validateIdentity(mode, 'registry entry');
    if (typeof mode.validate !== 'function') fail(`${mode.id}@${mode.version} requires validate()`);
    const key = `${mode.id}@${mode.version}`;
    if (entries.has(key)) fail(`registry repeats ${key}`);
    entries.set(key, Object.freeze({ ...mode }));
  }
  return Object.freeze({
    ids: Object.freeze([...entries.keys()].sort()),
    validate(input, { at = 'mode' } = {}) {
      validateIdentity(input, at);
      const key = `${input.id}@${input.version}`;
      const mode = entries.get(key);
      if (!mode) fail(`${at} selects unknown ${key}; available: ${[...entries.keys()].sort().join(', ')}`);
      return mode.validate(structuredClone(input), { at });
    },
  });
}

const sequentialMode = {
  id: 'sequential',
  version: '1.0.0',
  validate(value, { at }) {
    const fields = new Set(['id', 'version']);
    for (const key of Object.keys(value)) {
      if (!fields.has(key)) fail(`${at}.${key} is unknown for sequential mode`);
    }
    return { id: value.id, version: value.version };
  },
};

const dependencyMode = {
  id: 'dependency',
  version: '1.0.0',
  validate(value, { at }) {
    const fields = new Set(['id', 'version']);
    for (const key of Object.keys(value)) {
      if (!fields.has(key)) fail(`${at}.${key} is unknown for dependency mode`);
    }
    return { id: value.id, version: value.version };
  },
};

export const CAMPAIGN_MODE_REGISTRY = createCampaignModeRegistry([
  dependencyMode,
  sequentialMode,
]);

export function validateCampaignMode(value, options) {
  return CAMPAIGN_MODE_REGISTRY.validate(value, options);
}
