import assert from 'node:assert/strict';
import test from 'node:test';

import { evidenceNowMs } from '../src/evidence/evidence-timing.js';

test('evidence timestamps are epoch-shaped integers and never move backwards', () => {
  const first = evidenceNowMs();
  const second = evidenceNowMs();

  assert(Number.isInteger(first));
  assert(first > 1_000_000_000_000);
  assert(second >= first);
});
