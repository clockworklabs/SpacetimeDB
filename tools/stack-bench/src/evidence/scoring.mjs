// Pure scoring comparisons shared by the benchmark loop and its tests.
//
// A grading pass is evidence, not just a numerator and denominator. Losing a
// conclusive observation in a later pass must never look like an improvement:
// otherwise a fix can make a passing or failing criterion untestable and have
// that criterion disappear from the comparison.

import { criterionEvidence, evidenceDisposition } from './check-evidence.mjs';

function criterionIndex(bundle) {
  const index = new Map();
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

export function compareCriterionEvidence(before, after) {
  const previous = criterionIndex(before);
  const current = criterionIndex(after);
  let scoreBefore = 0;
  let scoreAfter = 0;
  let count = 0;
  let points = 0;
  const lostEvidence = [];
  const definitionChanges = [];
  const newlyConclusive = [];

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

export function formatRepairProgress(comparison, { before, beforeMax, after, afterMax }) {
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
