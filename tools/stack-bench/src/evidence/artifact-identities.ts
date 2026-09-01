import type { Dirent } from 'node:fs';

import { STACK_BENCH_ROOT } from '../package-root.js';
import { hashDirectory } from './provenance.js';

type UnknownRecord = Record<string, unknown>;

export interface ArtifactIdentity {
  id: string;
  version: string | null;
  sha256: string | null;
  state: string | null;
}

export interface EngineArtifactIdentity extends ArtifactIdentity {
  id: 'stack-bench';
  version: null;
  sha256: string;
  state: null;
}

export const ARTIFACT_IDENTITY_KEYS = Object.freeze([
  'engine', 'recipe', 'fixture', 'calibration', 'experiment', 'agentAdapter', 'stackAdapter',
] as const);

type ArtifactIdentityKey = typeof ARTIFACT_IDENTITY_KEYS[number];

export type ArtifactIdentities = Record<ArtifactIdentityKey, ArtifactIdentity | null> & {
  packs: ArtifactIdentity[];
};

const HASH = /^[a-f0-9]{64}$/;
let cachedEngineIdentity: EngineArtifactIdentity | null = null;

const isObject = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function fail(message: string): never {
  throw new Error(`invalid artifact: ${message}`);
}

function asObject(value: unknown, message: string): UnknownRecord {
  if (!isObject(value)) fail(message);
  return value;
}

function identity(value: unknown, at: string, requireComplete: boolean): ArtifactIdentity | null {
  if (value === null) return null;
  const candidate = asObject(value, `${at} must be an object or null`);
  const fields = ['id', 'version', 'sha256', 'state'] as const;
  for (const key of Object.keys(candidate)) {
    if (!fields.includes(key as typeof fields[number])) fail(`${at}.${key} is unknown`);
  }
  if (requireComplete) {
    for (const field of fields) {
      if (!Object.hasOwn(candidate, field)) fail(`${at}.${field} is required`);
    }
  }
  if (typeof candidate.id !== 'string' || !candidate.id) fail(`${at}.id must be a non-empty string`);
  if (candidate.version !== null && candidate.version !== undefined
    && (typeof candidate.version !== 'string' || !candidate.version)) fail(`${at}.version is invalid`);
  if (candidate.sha256 !== null && candidate.sha256 !== undefined
    && (typeof candidate.sha256 !== 'string' || !HASH.test(candidate.sha256))) {
    fail(`${at}.sha256 must be 64 lowercase hexadecimal characters`);
  }
  if (candidate.state !== null && candidate.state !== undefined
    && (typeof candidate.state !== 'string' || !candidate.state)) fail(`${at}.state is invalid`);
  return {
    id: candidate.id,
    version: candidate.version ?? null,
    sha256: candidate.sha256 ?? null,
    state: candidate.state ?? null,
  };
}

export function validateArtifactIdentities(
  value: unknown,
  {
    requireEngine = false,
    requireComplete = false,
    sortPacks = true,
  }: { requireEngine?: boolean; requireComplete?: boolean; sortPacks?: boolean } = {},
): ArtifactIdentities {
  const candidate = asObject(value, 'identities must be an object');
  const allowed = new Set([...ARTIFACT_IDENTITY_KEYS, 'packs']);
  for (const key of Object.keys(candidate)) if (!allowed.has(key)) fail(`identities.${key} is unknown`);
  if (requireComplete) {
    for (const key of allowed) {
      if (!Object.hasOwn(candidate, key)) fail(`identities.${key} is required`);
    }
  }
  const packsValue = candidate.packs ?? [];
  if (!Array.isArray(packsValue)) fail('identities.packs must be an array');
  const normalized: ArtifactIdentities = {
    engine: identity(candidate.engine ?? null, 'identities.engine', requireComplete),
    recipe: identity(candidate.recipe ?? null, 'identities.recipe', requireComplete),
    fixture: identity(candidate.fixture ?? null, 'identities.fixture', requireComplete),
    calibration: identity(candidate.calibration ?? null, 'identities.calibration', requireComplete),
    experiment: identity(candidate.experiment ?? null, 'identities.experiment', requireComplete),
    agentAdapter: identity(candidate.agentAdapter ?? null, 'identities.agentAdapter', requireComplete),
    stackAdapter: identity(candidate.stackAdapter ?? null, 'identities.stackAdapter', requireComplete),
    packs: packsValue.map((item, index) => {
      const pack = identity(item, `identities.packs[${index}]`, requireComplete);
      if (pack === null) fail(`identities.packs[${index}] must not be null`);
      return pack;
    }),
  };
  if (requireEngine && normalized.engine === null) fail('identities.engine is required');
  const packIds = new Set<string>();
  for (const pack of normalized.packs) {
    const key = `${pack.id}@${pack.version ?? ''}:${pack.sha256 ?? ''}`;
    if (packIds.has(key)) fail(`identities.packs duplicates ${key}`);
    packIds.add(key);
  }
  if (sortPacks) {
    normalized.packs.sort((a, b) => `${a.id}@${a.version ?? ''}`.localeCompare(`${b.id}@${b.version ?? ''}`));
  }
  return normalized;
}

export function currentEngineIdentity(): EngineArtifactIdentity {
  if (cachedEngineIdentity) return structuredClone(cachedEngineIdentity);
  const excludedRoots = new Set([
    'archive', 'local-notes', 'media', 'qualification-evidence', 'reference-apps', 'results',
    'tests', 'tracks', 'transcripts',
  ]);
  const exclude = (name: string, entry: Dirent): boolean => {
    const parts = name.split('/');
    if (parts.some(part => part.startsWith('.')) || excludedRoots.has(parts[0] ?? '')
      || parts.includes('node_modules')) return true;
    if (entry.isDirectory()) return false;
    if (name === 'dependency-manifest.json') return true;
    return !(/\.(?:ts|js|json|ya?ml|sh)$/.test(name) || /(?:^|\/)Dockerfile$/.test(name));
  };
  const executable = hashDirectory(STACK_BENCH_ROOT, { exclude });
  cachedEngineIdentity = { id: 'stack-bench', version: null, sha256: executable.sha256, state: null };
  return structuredClone(cachedEngineIdentity);
}

export function emptyArtifactIdentities(overrides: UnknownRecord = {}): ArtifactIdentities {
  return validateArtifactIdentities({ engine: currentEngineIdentity(), packs: [], ...overrides },
    { requireEngine: true });
}

export function recipeArtifactIdentities(
  recipeRelease: unknown,
  overrides: UnknownRecord = {},
): ArtifactIdentities {
  if (!recipeRelease) return emptyArtifactIdentities(overrides);
  const release = asObject(recipeRelease, 'recipe release must be an object');
  const components = release.components === undefined
    ? {} : asObject(release.components, 'recipe release components must be an object');
  const fixture = components.fixture == null
    ? null : asObject(components.fixture, 'recipe release fixture must be an object');
  const packsValue = components.packs ?? [];
  if (!Array.isArray(packsValue)) fail('recipe release packs must be an array');
  return emptyArtifactIdentities({
    recipe: { id: release.id, version: release.version,
      sha256: release.contentSha256, state: release.state },
    fixture: fixture ? {
      id: fixture.id,
      version: fixture.version,
      sha256: fixture.sha256 ?? null,
      state: fixture.state,
    } : null,
    packs: packsValue.map((value, index) => {
      const pack = asObject(value, `recipe release packs[${index}] must be an object`);
      return { id: pack.id, version: pack.version, sha256: pack.sha256 ?? null, state: pack.state };
    }),
    ...overrides,
  });
}
