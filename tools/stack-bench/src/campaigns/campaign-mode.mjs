export const CAMPAIGN_MODE_SCHEMA_VERSION = 1;

const ID = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const positiveInteger = (value, at) => {
  if (!Number.isSafeInteger(value) || value < 1) fail(`${at} must be a positive integer`);
  return value;
};

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
  version: '2.1.0',
  validate(value, { at }) {
    const fields = new Set(['id', 'version', 'strikes']);
    for (const key of Object.keys(value)) {
      if (!fields.has(key)) fail(`${at}.${key} is unknown for dependency mode`);
    }
    if (!object(value.strikes)) fail(`${at}.strikes must be an object`);
    const strikeFields = new Set(['default', 'levels']);
    for (const key of Object.keys(value.strikes)) {
      if (!strikeFields.has(key)) fail(`${at}.strikes.${key} is unknown`);
    }
    const levels = value.strikes.levels ?? {};
    if (!object(levels)) fail(`${at}.strikes.levels must be an object`);
    const normalizedLevels = {};
    for (const [level, budget] of Object.entries(levels)) {
      if (!/^[1-9]\d*$/.test(level)) fail(`${at}.strikes.levels.${level} has an invalid level`);
      normalizedLevels[level] = positiveInteger(budget, `${at}.strikes.levels.${level}`);
    }
    return { id: value.id, version: value.version, strikes: {
      ...(value.strikes.default === undefined ? {}
        : { default: positiveInteger(value.strikes.default, `${at}.strikes.default`) }),
      levels: normalizedLevels,
    } };
  },
};

export const CAMPAIGN_MODE_REGISTRY = createCampaignModeRegistry([
  dependencyMode,
  sequentialMode,
]);

export function validateCampaignMode(value, options) {
  return CAMPAIGN_MODE_REGISTRY.validate(value, options);
}
