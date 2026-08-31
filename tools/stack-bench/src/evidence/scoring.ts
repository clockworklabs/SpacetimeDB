// A later pass cannot improve by losing conclusive evidence.

import {
  criterionEvidence,
  evidenceDisposition,
  type CheckEvidenceStatus,
} from './check-evidence.js';

interface ScoredCriterion {
  id?: string;
  points?: number | null;
  evidence?: unknown;
}

interface ScoredFeature {
  id?: string | number | null;
  name?: string | null;
  criteria?: readonly ScoredCriterion[] | null;
}

interface ScoredSuite {
  features?: readonly ScoredFeature[] | null;
}

export interface EvidenceBundle {
  suites?: Readonly<Record<string, ScoredSuite | null | undefined>> | null;
}

interface IndexedCriterion {
  key: string;
  passed: boolean;
  measured: boolean;
  status: CheckEvidenceStatus;
  points: number;
}

export interface CriterionEvidenceComparison {
  before: number;
  after: number;
  count: number;
  points: number;
  lostEvidence: string[];
  definitionChanges: Array<{ key: string; before: number; after: number }>;
  newlyConclusive: string[];
}

export interface RepairScores {
  before: number;
  beforeMax: number;
  after: number;
  afterMax: number;
}

function criterionIndex(bundle: EvidenceBundle | null | undefined): Map<string, IndexedCriterion> {
  const index = new Map<string, IndexedCriterion>();
  for (const [suiteId, suite] of Object.entries(bundle?.suites ?? {})) {
    for (const feature of suite?.features ?? []) {
      const featureId = feature.id ?? feature.name;
      for (const criterion of feature.criteria ?? []) {
        const key = `${suiteId}/${featureId}/${criterion.id}`;
        if (index.has(key)) throw new Error(`duplicate criterion identity: ${key}`);
        const evidence = criterionEvidence(criterion);
        const disposition = evidenceDisposition(evidence);
        index.set(key, {
          key,
          passed: disposition.passed,
          measured: disposition.measured,
          status: evidence.status,
          points: criterion.points ?? 1,
        });
      }
    }
  }
  return index;
}

export function compareCriterionEvidence(
  before: EvidenceBundle | null | undefined,
  after: EvidenceBundle | null | undefined,
): CriterionEvidenceComparison {
  const previous = criterionIndex(before);
  const current = criterionIndex(after);
  let scoreBefore = 0;
  let scoreAfter = 0;
  let count = 0;
  let points = 0;
  const lostEvidence: string[] = [];
  const definitionChanges: CriterionEvidenceComparison['definitionChanges'] = [];
  const newlyConclusive: string[] = [];

  for (const [key, was] of previous) {
    const now = current.get(key);
    if (!was.measured) {
      if (now && now.measured) newlyConclusive.push(key);
      continue;
    }
    if (!now || !now.measured) {
      lostEvidence.push(key);
      continue;
    }
    if (now.points !== was.points) {
      definitionChanges.push({ key, before: was.points, after: now.points });
      continue;
    }
    count += 1;
    points += was.points;
    scoreBefore += was.passed ? was.points : 0;
    scoreAfter += now.passed ? now.points : 0;
  }

  return {
    before: scoreBefore,
    after: scoreAfter,
    count,
    points,
    lostEvidence,
    definitionChanges,
    newlyConclusive,
  };
}

export function formatRepairProgress(
  comparison: CriterionEvidenceComparison,
  { before, beforeMax, after, afterMax }: RepairScores,
): string {
  const shared = `${comparison.count} ${comparison.count === 1 ? 'criterion' : 'criteria'} measured in both rounds `
    + `(${comparison.before}/${comparison.points} points)`;
  const overall = `overall ${before}/${beforeMax} -> ${after}/${afterMax}`;
  if (comparison.newlyConclusive.length) {
    const count = comparison.newlyConclusive.length;
    return `no change among ${shared}; ${count} previously unavailable `
      + `${count === 1 ? 'criterion became' : 'criteria became'} measurable; ${overall}`;
  }
  return `no improvement among ${shared}; ${overall}`;
}
