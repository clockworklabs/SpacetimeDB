import {
  DEFAULT_DEPENDENCY_WORK_SELECTION,
  isDependencyWorkSelection,
} from '../progression/dependency-definition.js';

export const CAMPAIGN_MODE_SCHEMA_VERSION = 1;

const ID = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
type UnknownRecord = Record<string, unknown>;

export interface CampaignModeInput extends UnknownRecord {
  id: string;
}

export interface CampaignModeDefinition extends CampaignModeInput {
  validate(value: CampaignModeInput, options: { at: string }): CampaignModeInput;
}

export interface CampaignModeRegistry {
  ids: readonly string[];
  validate(input: unknown, options?: { at?: string }): CampaignModeInput;
}

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
function fail(message: string): never {
  throw new Error(`invalid campaign mode: ${message}`);
}

function validateIdentity(value: unknown, at: string): asserts value is CampaignModeInput {
  if (!object(value)) fail(`${at} must be an object`);
  if (typeof value.id !== 'string' || !ID.test(value.id)) fail(`${at}.id is invalid`);
}

export function createCampaignModeRegistry(modes: CampaignModeDefinition[]): CampaignModeRegistry {
  if (!Array.isArray(modes) || modes.length === 0) fail('registry requires at least one mode');
  const entries = new Map<string, CampaignModeDefinition>();
  for (const mode of modes) {
    validateIdentity(mode, 'registry entry');
    if (typeof mode.validate !== 'function') fail(`${mode.id} requires validate()`);
    if (entries.has(mode.id)) fail(`registry repeats ${mode.id}`);
    entries.set(mode.id, Object.freeze({ ...mode }));
  }
  return Object.freeze({
    ids: Object.freeze([...entries.keys()].sort()),
    validate(input: unknown, { at = 'mode' }: { at?: string } = {}) {
      validateIdentity(input, at);
      const mode = entries.get(input.id);
      if (!mode) fail(`${at} selects unknown ${input.id}; available: ${[...entries.keys()].sort().join(', ')}`);
      return mode.validate(structuredClone(input), { at });
    },
  });
}

const sequentialMode = {
  id: 'sequential',
  validate(value: CampaignModeInput, { at }: { at: string }): CampaignModeInput {
    const fields = new Set(['id']);
    for (const key of Object.keys(value)) {
      if (!fields.has(key)) fail(`${at}.${key} is unknown for sequential mode`);
    }
    return { id: value.id };
  },
};

const dependencyMode = {
  id: 'dependency',
  validate(value: CampaignModeInput, { at }: { at: string }): CampaignModeInput {
    const fields = new Set(['id', 'workSelection']);
    for (const key of Object.keys(value)) {
      if (!fields.has(key)) fail(`${at}.${key} is unknown for dependency mode`);
    }
    const workSelection = value.workSelection ?? DEFAULT_DEPENDENCY_WORK_SELECTION;
    if (!isDependencyWorkSelection(workSelection)) {
      fail(`${at}.workSelection must be "feature", "progressive", or "all-at-once"`);
    }
    return { id: value.id, workSelection };
  },
};

export const CAMPAIGN_MODE_REGISTRY = createCampaignModeRegistry([
  dependencyMode,
  sequentialMode,
]);

export function validateCampaignMode(value: unknown, options?: { at?: string }): CampaignModeInput {
  return CAMPAIGN_MODE_REGISTRY.validate(value, options);
}
