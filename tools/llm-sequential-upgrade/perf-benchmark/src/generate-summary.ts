// Reads all per-scenario JSON results from a directory and emits summary.md
// and summary.json side-by-side comparing PG vs STDB.

import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { ScenarioResult } from './metrics.ts';

function loadAll(dir: string): ScenarioResult[] {
  const out: ScenarioResult[] = [];
  for (const f of readdirSync(dir)) {
    if (!f.endsWith('.json') || f === 'summary.json') continue;
    out.push(JSON.parse(readFileSync(join(dir, f), 'utf8')) as ScenarioResult);
  }
  return out;
}

function fmt(n: number, digits = 1): string {
  return n.toFixed(digits);
}

function main(): void {
  const args = process.argv.slice(2);
  const pgDir = args[0] ?? 'results/full-pg';
  const stdbDir = args[1] ?? 'results/full-stdb';
  const outDir = args[2] ?? 'results';

  const pg = loadAll(pgDir);
  const stdb = loadAll(stdbDir);
  const byScenario = (rs: ScenarioResult[], s: string): ScenarioResult | undefined =>
    rs.find((r) => r.scenario === s);

  const scenarios = ['stress-throughput', 'realistic-chat'] as const;

  const lines: string[] = [];
  lines.push('# Perf Benchmark Summary - PG vs STDB Chat Apps');
  lines.push('');
  lines.push('Runtime performance of the **Level 12 chat apps the LLM built** in the sequential upgrade benchmark.');
  lines.push('Both apps run on the same dev machine against a local DB. Numbers reflect what shipped, not the theoretical ceiling of either backend.');
  lines.push('');

  for (const sc of scenarios) {
    const p = byScenario(pg, sc);
    const s = byScenario(stdb, sc);
    if (!p && !s) continue;
    lines.push(`## ${sc}`);
    lines.push('');
    lines.push('| Metric | PostgreSQL | SpacetimeDB |');
    lines.push('|---|---|---|');
    lines.push(`| Sustained throughput (msgs/sec) | ${p ? fmt(p.msgsPerSec) : '-'} | ${s ? fmt(s.msgsPerSec) : '-'} |`);
    lines.push(`| Messages received | ${p?.received ?? '-'} | ${s?.received ?? '-'} |`);
    lines.push(`| Fan-out latency p50 (ms) | ${p ? fmt(p.fanoutLatencyMs.p50) : '-'} | ${s ? fmt(s.fanoutLatencyMs.p50) : '-'} |`);
    lines.push(`| Fan-out latency p99 (ms) | ${p ? fmt(p.fanoutLatencyMs.p99) : '-'} | ${s ? fmt(s.fanoutLatencyMs.p99) : '-'} |`);
    if (p?.ackLatencyMs.count || s?.ackLatencyMs.count) {
      lines.push(`| Ack latency p50 (ms) | ${p?.ackLatencyMs.count ? fmt(p.ackLatencyMs.p50) : '-'} | ${s?.ackLatencyMs.count ? fmt(s.ackLatencyMs.p50) : '-'} |`);
      lines.push(`| Ack latency p99 (ms) | ${p?.ackLatencyMs.count ? fmt(p.ackLatencyMs.p99) : '-'} | ${s?.ackLatencyMs.count ? fmt(s.ackLatencyMs.p99) : '-'} |`);
    }
    if (p?.notes) lines.push(`\n**PG note:** ${p.notes}`);
    if (s?.notes) lines.push(`\n**STDB note:** ${s.notes}`);
    lines.push('');
  }

  const stress = { pg: byScenario(pg, 'stress-throughput'), stdb: byScenario(stdb, 'stress-throughput') };
  if (stress.pg && stress.stdb) {
    const ratio = stress.stdb.msgsPerSec / stress.pg.msgsPerSec;
    lines.push('## Headline');
    lines.push('');
    lines.push(`Under stress, the SpacetimeDB app delivered **${fmt(ratio, 0)}x the throughput** of the PostgreSQL app `);
    lines.push(`(${fmt(stress.stdb.msgsPerSec)} vs ${fmt(stress.pg.msgsPerSec)} msgs/sec)`);
    lines.push(`with comparable p99 fan-out latency (${fmt(stress.stdb.fanoutLatencyMs.p99)}ms vs ${fmt(stress.pg.fanoutLatencyMs.p99)}ms).`);
    lines.push('');
    lines.push('The PG send_message handler serializes 5 DB queries per message (ban check, membership check,');
    lines.push('`lastSeen` update, insert, roomMembers query for notifications) - all awaited, no batching.');
    lines.push('The SpacetimeDB reducer does a single transaction. **This is what shipped from the same prompt** -');
    lines.push('the LLM reached for a familiar REST pattern on PG and a minimal reducer on STDB, and the');
    lines.push("generated code's structure dominates the throughput gap.");
  }

  writeFileSync(join(outDir, 'summary.md'), lines.join('\n'));

  const summary = {
    pg: Object.fromEntries(pg.map((r) => [r.scenario, r])),
    stdb: Object.fromEntries(stdb.map((r) => [r.scenario, r])),
    headline: stress.pg && stress.stdb ? {
      stressMsgsPerSecPg: stress.pg.msgsPerSec,
      stressMsgsPerSecStdb: stress.stdb.msgsPerSec,
      stressRatio: stress.stdb.msgsPerSec / stress.pg.msgsPerSec,
      stressP99FanoutPg: stress.pg.fanoutLatencyMs.p99,
      stressP99FanoutStdb: stress.stdb.fanoutLatencyMs.p99,
    } : null,
  };
  writeFileSync(join(outDir, 'summary.json'), JSON.stringify(summary, null, 2));

  console.log(`Wrote ${join(outDir, 'summary.md')} and summary.json`);
}

main();
