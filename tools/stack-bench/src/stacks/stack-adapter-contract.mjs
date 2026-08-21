export const STACK_ADAPTER_SCHEMA_VERSION = 1;
export const STACK_CAPABILITY_SCHEMA_VERSION = 1;

export const STACK_ENGINE_REQUIREMENTS = Object.freeze({
  ports: Object.freeze(['allocations', 'for-run']),
  lease: Object.freeze(['prepare', 'validate-resources']),
  reset: Object.freeze(['requires-reseed']),
  database: Object.freeze(['prepare']),
  lifecycle: Object.freeze(['activate']),
  grading: Object.freeze(['context']),
  'named-action': Object.freeze(['request']),
  teardown: Object.freeze(['host']),
  'run-policy': Object.freeze(['reset-enabled', 'sandbox-probe-required', 'product-review-enabled',
    'product-review-comparisons', 'supervisor-env', 'retain-host-supported']),
  agent: Object.freeze(['connection-url', 'minimal-guidance-supported', 'default-skills',
    'linux-cli-required', 'setup-metadata', 'server-directory', 'find-database-urls']),
  'build-container': Object.freeze(['plan']),
  orchestrator: Object.freeze(['config']),
});

const ADAPTER_FIELDS = new Set(['schemaVersion', 'id', 'version', 'capabilities']);
const CAPABILITY_FIELDS = new Set(['schemaVersion', 'id', 'version', 'operations', 'execute']);
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;

export class StackCapabilityUnsupportedError extends Error {
  constructor(message) {
    super(message);
    this.name = 'StackCapabilityUnsupportedError';
  }
}

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const nonEmpty = value => typeof value === 'string' && value.trim().length > 0;

function strict(value, fields, at) {
  if (!object(value)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
}

function identifier(value, at) {
  if (!nonEmpty(value) || !ID.test(value)) throw new Error(`${at} is invalid`);
}

function version(value, at) {
  if (!nonEmpty(value) || !VERSION.test(value)) throw new Error(`${at} is invalid`);
}

export function defineStackCapability(value, at = 'stack capability') {
  strict(value, CAPABILITY_FIELDS, at);
  if (value.schemaVersion !== STACK_CAPABILITY_SCHEMA_VERSION) {
    throw new Error(`${at}.schemaVersion is unsupported`);
  }
  identifier(value.id, `${at}.id`);
  version(value.version, `${at}.version`);
  if (!Array.isArray(value.operations) || value.operations.length === 0) {
    throw new Error(`${at}.operations must be a non-empty array`);
  }
  value.operations.forEach((operation, index) => identifier(operation, `${at}.operations[${index}]`));
  if (new Set(value.operations).size !== value.operations.length) {
    throw new Error(`${at}.operations contains duplicates`);
  }
  if (typeof value.execute !== 'function') throw new Error(`${at}.execute must be a function`);
  const operations = Object.freeze([...value.operations].sort());
  return Object.freeze({ ...value, operations });
}

export function defineStackAdapter(value) {
  strict(value, ADAPTER_FIELDS, 'stack adapter');
  if (value.schemaVersion !== STACK_ADAPTER_SCHEMA_VERSION) {
    throw new Error(`stack adapter ${value.id ?? '<unknown>'} uses unsupported schema ${value.schemaVersion}`);
  }
  identifier(value.id, 'stack adapter.id');
  version(value.version, `stack adapter ${value.id}.version`);
  if (!object(value.capabilities)) throw new Error(`stack adapter ${value.id}.capabilities must be an object`);
  const capabilities = {};
  for (const [name, provider] of Object.entries(value.capabilities)) {
    identifier(name, `stack adapter ${value.id} capability name`);
    capabilities[name] = defineStackCapability(provider,
      `stack adapter ${value.id}.capabilities.${name}`);
    if (capabilities[name].id !== `${value.id}.${name}`) {
      throw new Error(`stack adapter ${value.id}.capabilities.${name}.id must be ${value.id}.${name}`);
    }
  }
  if (Object.keys(capabilities).length === 0) {
    throw new Error(`stack adapter ${value.id} must declare at least one capability`);
  }
  return Object.freeze({ ...value, capabilities: Object.freeze(capabilities) });
}

export function executeStackCapability(adapter, capabilityName, operation, input = {}) {
  const provider = adapter.capabilities[capabilityName];
  if (!provider) {
    throw new StackCapabilityUnsupportedError(
      `stack adapter ${adapter.id} does not support capability ${capabilityName}`);
  }
  if (!provider.operations.includes(operation)) {
    throw new StackCapabilityUnsupportedError(
      `stack adapter ${adapter.id} capability ${capabilityName} does not support operation ${operation}`);
  }
  return provider.execute(operation, input);
}

export function createStackAdapterRegistry(adapters) {
  if (!Array.isArray(adapters)) throw new Error('stack adapter registry requires an array');
  const entries = new Map();
  for (const source of adapters) {
    const adapter = defineStackAdapter(source);
    for (const [capability, operations] of Object.entries(STACK_ENGINE_REQUIREMENTS)) {
      const provider = adapter.capabilities[capability];
      if (!provider) throw new Error(`stack adapter ${adapter.id} is missing required capability ${capability}`);
      for (const operation of operations) {
        if (!provider.operations.includes(operation)) {
          throw new Error(`stack adapter ${adapter.id} capability ${capability} is missing required operation ${operation}`);
        }
      }
    }
    if (entries.has(adapter.id)) throw new Error(`duplicate stack adapter ${adapter.id}`);
    entries.set(adapter.id, adapter);
  }
  const ids = Object.freeze([...entries.keys()].sort());
  return Object.freeze({
    ids,
    get(id) {
      const adapter = entries.get(id);
      if (!adapter) throw new Error(`unknown stack adapter ${JSON.stringify(id)}`);
      return adapter;
    },
  });
}
