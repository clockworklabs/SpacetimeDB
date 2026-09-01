import type { Dirent } from 'node:fs';
import { z } from 'zod';

import { STACK_BENCH_ROOT } from '../package-root.js';
import { hashDirectory } from './provenance.js';
import { formatZodError } from '../zod-error.js';

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
const identityShape = {
  id: z.string().min(1),
  version: z.string().min(1).nullable().optional(),
  sha256: z.string().regex(HASH).nullable().optional(),
  state: z.string().min(1).nullable().optional(),
};
const identitySchema = z.strictObject(identityShape);
const completeIdentitySchema = z.strictObject({
  id: identityShape.id,
  version: identityShape.version.unwrap(),
  sha256: identityShape.sha256.unwrap(),
  state: identityShape.state.unwrap(),
});
const optionalIdentitySchema = identitySchema.nullable().optional();
const identitiesSchema = z.strictObject({
  engine: optionalIdentitySchema,
  recipe: optionalIdentitySchema,
  fixture: optionalIdentitySchema,
  calibration: optionalIdentitySchema,
  experiment: optionalIdentitySchema,
  agentAdapter: optionalIdentitySchema,
  stackAdapter: optionalIdentitySchema,
  packs: z.array(identitySchema).optional(),
});
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
  const fields = ['id', 'version', 'sha256', 'state'] as const;
  if (requireComplete) {
    const candidate = asObject(value, `${at} must be an object or null`);
    for (const field of fields) {
      if (!Object.hasOwn(candidate, field)) fail(`${at}.${field} is required`);
    }
  }
  const parsed = (requireComplete ? completeIdentitySchema : identitySchema).safeParse(value);
  if (!parsed.success) {
    fail(formatZodError(parsed.error, at));
  }
  const candidate = parsed.data;
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
  const allowed = new Set([...ARTIFACT_IDENTITY_KEYS, 'packs']);
  if (requireComplete) {
    const candidate = asObject(value, 'identities must be an object');
    for (const key of allowed) {
      if (!Object.hasOwn(candidate, key)) fail(`identities.${key} is required`);
    }
  }
  const parsed = identitiesSchema.safeParse(value);
  if (!parsed.success) {
    fail(formatZodError(parsed.error, 'identities'));
  }
  const candidate = parsed.data;
  const packsValue = candidate.packs ?? [];
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
