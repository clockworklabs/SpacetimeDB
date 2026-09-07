import assert from 'node:assert/strict';
import { test } from 'node:test';
import { bigClimb, climb } from '../../dashboard/public/climb.js';
import { graph } from '../../dashboard/public/graph.js';

test('score charts explain missing grades and preserve circular marker geometry', () => {
  assert.match(climb([]), /Awaiting first grade/);
  assert.match(bigClimb([], String), /Awaiting first grade/);
  const series = [{ score: 1, max: 2, level: 1, unaided: true }];
  for (const html of [climb(series), bigClimb(series, String)]) {
    assert.match(html, /preserveAspectRatio="xMidYMid meet"/);
    assert.match(html, /1 \/ 2 points/);
    assert.doesNotMatch(html, /NaN|Infinity/);
  }
  assert.match(bigClimb(series, String), /aria-label="Score history" tabindex="0"/);
});

test('an empty feature graph states that no graph is available', () => {
  assert.match(graph({ key: 'empty', depths: [], questlines: [], nodes: [], stacks: [] }, []),
    /No feature graph is available/);
});
