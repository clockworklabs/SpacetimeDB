#!/usr/bin/env node

/**
 * Generates a BENCHMARK_REPORT.md by aggregating cost-summary.json files
 * from a completed benchmark run.
 *
 * Usage:
 *   node generate-report.mjs <run-base-dir>
 *   node generate-report.mjs sequential-upgrade/sequential-upgrade-20260402
 *
 * Reads:  telemetry subdirs for cost-summary.json
 * Reads:  results subdirs for GRADING_RESULTS.md (feature scores)
 * Writes: BENCHMARK_REPORT.md in the run base directory
 */

import fs from 'fs';
import path from 'path';

const runBaseDir = process.argv[2];
if (!runBaseDir) {
  console.error('Usage: node generate-report.mjs <run-base-dir>');
  process.exit(1);
}

// Find all cost-summary.json files
const telemetryDir = path.join(runBaseDir, 'telemetry');
if (!fs.existsSync(telemetryDir)) {
  console.error(`Telemetry directory not found: ${telemetryDir}`);
  process.exit(1);
}

const summaries = [];
for (const entry of fs.readdirSync(telemetryDir)) {
  const summaryPath = path.join(telemetryDir, entry, 'cost-summary.json');
  if (fs.existsSync(summaryPath)) {
    const data = JSON.parse(fs.readFileSync(summaryPath, 'utf-8'));
    data._dir = entry;
    summaries.push(data);
  }
}

if (summaries.length === 0) {
  console.error('No cost-summary.json files found in telemetry subdirectories.');
  console.error('Run parse-telemetry.mjs with --extract-raw first.');
  process.exit(1);
}

// Group by backend
const byBackend = {};
for (const s of summaries) {
  const backend = s.backend || 'unknown';
  if (!byBackend[backend]) byBackend[backend] = [];
  byBackend[backend].push(s);
}

// Sort each backend's summaries by level
for (const backend of Object.keys(byBackend)) {
  byBackend[backend].sort((a, b) => (a.level || 0) - (b.level || 0));
}

// Calculate totals per backend (sum of final run per level)
function calcTotals(runs) {
  // Group by level, take the last run per level (final successful)
  const byLevel = {};
  for (const r of runs) {
    const level = r.level || 0;
    byLevel[level] = r; // last one wins
  }
  const levels = Object.values(byLevel);
  return {
    totalCost: levels.reduce((s, r) => s + (r.totalCostUsd || 0), 0),
    totalCalls: levels.reduce((s, r) => s + (r.apiCalls || 0), 0),
    totalTokens: levels.reduce((s, r) => s + (r.totalTokens || 0), 0),
    totalDuration: levels.reduce((s, r) => s + (r.totalDurationSec || 0), 0),
    levelCount: levels.length,
    levels,
  };
}

// Read GRADING_RESULTS.md for feature scores
function readGradingScores(backend) {
  const resultsDir = path.join(runBaseDir, 'results', backend);
  if (!fs.existsSync(resultsDir)) return null;

  const appDirs = fs.readdirSync(resultsDir)
    .filter(d => d.startsWith('chat-app-'))
    .map(d => path.join(resultsDir, d))
    .filter(d => fs.statSync(d).isDirectory());

  if (appDirs.length === 0) return null;

  // Take the most recent app dir
  const appDir = appDirs.sort().pop();
  const gradingPath = path.join(appDir, 'GRADING_RESULTS.md');
  if (!fs.existsSync(gradingPath)) return null;

  const content = fs.readFileSync(gradingPath, 'utf-8');

  // Extract total score from "**TOTAL** | **N** | **M**"
  const totalMatch = content.match(/\*\*TOTAL\*\*.*?\*\*(\d+)\*\*.*?\*\*(\d+)\*\*/);
  if (totalMatch) {
    return { max: parseInt(totalMatch[1]), score: parseInt(totalMatch[2]) };
  }

  // Fallback: look for "Total Feature Score" in metrics
  const scoreMatch = content.match(/Total Feature Score.*?(\d+)\s*\/\s*(\d+)/);
  if (scoreMatch) {
    return { score: parseInt(scoreMatch[1]), max: parseInt(scoreMatch[2]) };
  }

  return null;
}

// Count lines of code in app dir
function countLoc(backend) {
  const resultsDir = path.join(runBaseDir, 'results', backend);
  if (!fs.existsSync(resultsDir)) return null;

  const appDirs = fs.readdirSync(resultsDir)
    .filter(d => d.startsWith('chat-app-'))
    .map(d => path.join(resultsDir, d))
    .filter(d => fs.statSync(d).isDirectory());

  if (appDirs.length === 0) return null;
  const appDir = appDirs.sort().pop();

  let backendLoc = 0;
  let frontendLoc = 0;

  function countLines(dir) {
    if (!fs.existsSync(dir)) return 0;
    let total = 0;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === '.vite' ||
          entry.name === 'module_bindings' || entry.name.startsWith('level-')) continue;
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        total += countLines(fullPath);
      } else if (/\.(ts|tsx|js|jsx)$/.test(entry.name)) {
        total += fs.readFileSync(fullPath, 'utf-8').split('\n').length;
      }
    }
    return total;
  }

  // SpacetimeDB backend
  const stdbBackend = path.join(appDir, 'backend', 'spacetimedb', 'src');
  if (fs.existsSync(stdbBackend)) {
    backendLoc = countLines(stdbBackend);
  }

  // PostgreSQL backend
  const pgServer = path.join(appDir, 'server');
  if (fs.existsSync(pgServer)) {
    backendLoc = countLines(pgServer);
  }

  // Frontend
  const clientSrc = path.join(appDir, 'client', 'src');
  if (fs.existsSync(clientSrc)) {
    frontendLoc = countLines(clientSrc);
  }

  return { backendLoc, frontendLoc, totalLoc: backendLoc + frontendLoc };
}

// ─── Generate report ────────────────────────────────────────────────────────

const backends = Object.keys(byBackend);
const date = new Date().toISOString().slice(0, 10);
const variant = summaries[0]?.variant || 'unknown';
const rules = summaries[0]?.rules || 'unknown';

let report = `# LLM Cost-to-Done Benchmark Report

**Generated:** ${date}
**Variant:** ${variant}
**Rules:** ${rules}

---

## Summary

`;

if (backends.length >= 2) {
  const totals = {};
  const scores = {};
  const locs = {};
  for (const b of backends) {
    totals[b] = calcTotals(byBackend[b]);
    scores[b] = readGradingScores(b);
    locs[b] = countLoc(b);
  }

  const [b1, b2] = backends;
  const t1 = totals[b1];
  const t2 = totals[b2];
  const costDelta = ((t2.totalCost - t1.totalCost) / t2.totalCost * 100).toFixed(0);
  const cheaper = t1.totalCost < t2.totalCost ? b1 : b2;

  report += `| | ${b1} | ${b2} | Delta |
|--|-----|-----|-------|
| **Total LLM cost** | **$${t1.totalCost.toFixed(2)}** | **$${t2.totalCost.toFixed(2)}** | ${cheaper} ${Math.abs(Number(costDelta))}% cheaper |
| **API calls** | ${t1.totalCalls} | ${t2.totalCalls} | |
| **Total tokens** | ${t1.totalTokens.toLocaleString()} | ${t2.totalTokens.toLocaleString()} | |
| **Duration** | ${(t1.totalDuration / 60).toFixed(1)} min | ${(t2.totalDuration / 60).toFixed(1)} min | |
`;

  if (scores[b1] && scores[b2]) {
    report += `| **Feature score** | ${scores[b1].score}/${scores[b1].max} | ${scores[b2].score}/${scores[b2].max} | |\n`;
  }

  if (locs[b1] && locs[b2]) {
    report += `| **Backend LOC** | ${locs[b1].backendLoc} | ${locs[b2].backendLoc} | |\n`;
    report += `| **Frontend LOC** | ${locs[b1].frontendLoc} | ${locs[b2].frontendLoc} | |\n`;
    report += `| **Total LOC** | ${locs[b1].totalLoc} | ${locs[b2].totalLoc} | |\n`;
  }
} else {
  const b = backends[0];
  const t = calcTotals(byBackend[b]);
  const s = readGradingScores(b);

  report += `| Metric | Value |
|--------|-------|
| **Backend** | ${b} |
| **Total LLM cost** | $${t.totalCost.toFixed(2)} |
| **API calls** | ${t.totalCalls} |
| **Total tokens** | ${t.totalTokens.toLocaleString()} |
| **Duration** | ${(t.totalDuration / 60).toFixed(1)} min |
`;
  if (s) report += `| **Feature score** | ${s.score}/${s.max} |\n`;
}

// Per-level breakdown
report += `\n---\n\n## Per-Level Cost Breakdown\n\n`;

for (const backend of backends) {
  report += `### ${backend}\n\n`;
  report += `| Level | Cost | API Calls | Duration |\n`;
  report += `|-------|------|-----------|----------|\n`;

  const runs = byBackend[backend];
  const byLevel = {};
  for (const r of runs) {
    byLevel[r.level || 0] = r;
  }

  for (const [level, r] of Object.entries(byLevel).sort((a, b) => a[0] - b[0])) {
    report += `| ${level} | $${(r.totalCostUsd || 0).toFixed(2)} | ${r.apiCalls || 0} | ${((r.totalDurationSec || 0) / 60).toFixed(1)} min |\n`;
  }

  const t = calcTotals(runs);
  report += `| **Total** | **$${t.totalCost.toFixed(2)}** | **${t.totalCalls}** | **${(t.totalDuration / 60).toFixed(1)} min** |\n\n`;
}

report += `---\n\n*Generated by generate-report.mjs*\n`;

const outputPath = path.join(runBaseDir, 'BENCHMARK_REPORT.md');
fs.writeFileSync(outputPath, report);
console.log(`Report written to: ${outputPath}`);
console.log(`Backends: ${backends.join(', ')}`);
for (const b of backends) {
  const t = calcTotals(byBackend[b]);
  console.log(`  ${b}: $${t.totalCost.toFixed(2)} (${t.totalCalls} calls, ${(t.totalDuration / 60).toFixed(1)} min)`);
}
