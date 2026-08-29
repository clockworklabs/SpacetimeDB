export interface ReferenceFixture {
  id: string;
  backend: string;
  track: string;
  level: number;
  status: string;
  recipes?: string[];
  actionLevels?: number[];
  targetPath?: string;
  [key: string]: unknown;
}

export interface ReferenceRegistry {
  fixtures: ReferenceFixture[];
  [key: string]: unknown;
}

export interface ReferenceFixtureSelector {
  backend?: string;
  track?: string;
  level?: number;
  recipe?: string;
}

export function selectReferenceFixture(
  registry: ReferenceRegistry,
  selector?: ReferenceFixtureSelector,
): ReferenceFixture;

export function loadReferenceRegistry(path?: string): ReferenceRegistry;

export function validateReferenceRegistry(
  registry: unknown,
  options?: { root?: string },
): { ok: boolean; issues: string[] };
