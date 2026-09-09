import assert from 'node:assert/strict';
import { test } from 'node:test';
import { bigClimb } from '../../dashboard/public/climb.js';
import { graph } from '../../dashboard/public/graph.js';

test('score charts explain missing grades and preserve circular marker geometry', () => {
  assert.match(bigClimb([], String), /Awaiting first grade/);
  const series = [{ score: 1, max: 2, level: 1, unaided: true }];
  const html = bigClimb(series, String);
  assert.match(html, /preserveAspectRatio="xMidYMid meet"/);
  assert.match(html, /1 \/ 2 points/);
  assert.match(html, /Before repairs at this level; earlier fixes and feedback retained/);
  assert.doesNotMatch(html, /NaN|Infinity/);
  assert.match(html, /aria-label="Score history" tabindex="0"/);
});

test('an empty feature graph states that no graph is available', () => {
  assert.match(graph({ key: 'empty', depths: [], questlines: [], nodes: [], stacks: [] }, []),
    /No feature graph is available/);
});

for (const count of [1, 4, 6]) {
  test(`feature graph fits all ${count} stack markers without overlapping labels or columns`, () => {
    const html = graph({
      key: 'dynamic-stacks', depths: [1, 2], stacks: [],
      questlines: [{ id: 'shop', title: 'Shop', nodes: ['catalog', 'cart'] }],
      nodes: [
        { id: 'catalog', title: 'Product catalog', questline: 'shop', depth: 1, dependencies: [] },
        { id: 'cart', title: 'Shopping cart', questline: 'shop', depth: 2, dependencies: ['catalog'] },
      ],
    }, Array.from({ length: count }, (_, index) => ({ stack: `stack-${index}`, statuses: ['passed', 'active'] })));
    const groups = [...html.matchAll(/<g class="n">([\s\S]*?)<\/g>/g)];
    assert.equal(groups.length, 2);
    assert.equal((html.match(/class="passed-check"/g) ?? []).length, count);
    let previousRight = 0;
    for (const group of groups) {
      const rect = /<rect x="([\d.]+)" y="[\d.]+" width="([\d.]+)"/.exec(group[1]!);
      assert.ok(rect);
      const left = Number(rect[1]);
      const right = left + Number(rect[2]);
      assert.ok(left > previousRight, 'depth columns must not overlap');
      const markers = [...group[1]!.matchAll(/<circle[^>]*cx="([\d.]+)"[^>]*r="([\d.]+)"/g)];
      assert.equal(markers.length, count);
      let previousMarkerRight = left + 170; // Space reserved for the truncated feature label.
      for (const marker of markers) {
        const center = Number(marker[1]);
        const radius = Number(marker[2]);
        assert.ok(center - radius > previousMarkerRight, 'markers must clear labels and preceding markers');
        assert.ok(center + radius < right, 'markers must stay inside the feature node');
        previousMarkerRight = center + radius;
      }
      previousRight = right;
    }
    const canvas = /viewBox="0 0 ([\d.]+)/.exec(html);
    assert.ok(canvas && Number(canvas[1]) > previousRight, 'canvas must contain the final node');
    for (let index = 0; index < count; index += 1) assert.match(html, new RegExp(`stack-${index}`));
  });
}

import { progressChart } from '../../dashboard/public/progress-chart.js';
import type { CampaignSheet, CampaignProgression } from '../../dashboard/dashboard-views.js';

test('time chart uses measured elapsed time, preserves regressions, and labels excluded runs', () => {
  const sheet = { key: 'test', stacks: [{ stack: 'postgres', attempts: [{ id: 'a', repetition: 1,
    executionStartedAt: '2026-09-08T00:00:00Z', excluded: 'Provider failure' }] }] } as CampaignSheet;
  const progression = { stacks: [{ stack: 'postgres', attemptId: 'a', steps: [
    { completedAt: null, completion: 1 },
    { completedAt: '2026-09-08T00:01:00Z', completion: 0.75 },
    { completedAt: '2026-09-08T00:02:00Z', completion: 0.5 },
  ] }] } as CampaignProgression;
  const html = progressChart(sheet, progression);
  assert.match(html, /M48 190 L498 70 L948 110/);
  assert.match(html, /Run start; no checks graded/);
  assert.match(html, /Rep 1 · 50% · Excluded/);
  assert.doesNotMatch(html, /NaN|Infinity/);
  assert.match(progressChart(sheet, null), /Awaiting first timed grade/);
  for (const [stack, color] of Object.entries({ spacetime: '#4cf490', mongodb: '#b45af2', postgres: '#336791' })) {
    sheet.stacks[0]!.stack = stack;
    progression.stacks[0]!.stack = stack;
    assert.ok(progressChart(sheet, progression).includes(`stroke="${color}"`));
  }
});


test('cost chart uses cumulative checkpoint costs, labels bounds, and omits unknowns', () => {
  const sheet = { key: 'test', stacks: [{ stack: 'postgres', attempts: [{ id: 'a', repetition: 1,
    executionStartedAt: '2026-09-08T00:00:00Z', excluded: null }] }] } as CampaignSheet;
  const progression = { key: 'test', depths: [], questlines: [], nodes: [],
    stacks: [{ stack: 'postgres', attemptId: 'a', updatedAt: '', steps: [], costs: [
    { completedAt: '2026-09-08T00:01:00Z', cost: { status: 'exact', costUsd: 2 } },
    { completedAt: '2026-09-08T00:01:30Z', cost: { status: 'unknown', costUsd: null } },
    { completedAt: '2026-09-08T00:02:00Z', cost: { status: 'upper-bound', costUsd: 4 } },
  ] }] } as CampaignProgression;
  const html = progressChart(sheet, progression, 'cost', 'graph');
  assert.match(html, /M80 190 L514 110 L948 30/);
  assert.match(html, /Rep 1 · ≤\$4.00/);
  assert.match(html, /Run start; no recorded cost/);
  assert.match(html, /questlines=graph&amp;chart=completion/);
  assert.doesNotMatch(html, /NaN|Infinity/);
  assert.match(progressChart(sheet, null, 'cost'), /Awaiting first timed cost receipt/);
});

test('chart filters individual runs without changing the scale or hiding pending controls', () => {
  const sheet = { key: 'test', stacks: [{ stack: 'custom-stack', attempts: [1, 2, 3].map(repetition => ({
    id: `run-${repetition}`, repetition, executionStartedAt: '2026-09-08T00:00:00Z', excluded: null,
  })) }] } as CampaignSheet;
  const progression = { stacks: [1, 2].map(repetition => ({ stack: 'custom-stack', attemptId: `run-${repetition}`,
    steps: [{ completedAt: `2026-09-08T00:0${repetition}:00Z`, completion: repetition / 4 }],
  })) } as CampaignProgression;
  const html = progressChart(sheet, progression, 'completion', 'grid', new Set(['run-2']));
  assert.match(html, /data-chart-stack="custom-stack" aria-pressed="mixed"/);
  assert.match(html, /class="progress-series" data-chart-series="run-1"/);
  assert.doesNotMatch(html, /class="progress-series" data-chart-series="run-2"|stroke-dasharray| style=/);
  assert.match(html, /M48 190 L498 150/); // Retains the two-minute extent of the hidden run.
  assert.match(html, /Rep 3 · Pending/);
  const empty = progressChart(sheet, progression, 'completion', 'grid', new Set(['run-1', 'run-2', 'run-3']));
  assert.match(empty, /Select a run/);
  assert.match(empty, /data-chart-run="run-1"/); // Controls remain available to restore runs.
});
