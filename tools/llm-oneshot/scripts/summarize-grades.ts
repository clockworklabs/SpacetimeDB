#!/usr/bin/env tsx
/**
 * Grade Summary Script for LLM One-Shot Benchmarks
 *
 * Aggregates all GRADING_RESULTS.md files and generates:
 * - GRADE_SUMMARY.md (executive summary)
 * - grades.json (structured data for websites)
 * - Per-app summaries in {app}/
 *
 * Output is written to: docs/llms/oneshots/
 *
 * Usage:
 *   pnpm run summarize
 *   pnpm run summarize -- --app chat-app
 *   pnpm run summarize -- --llm opus-4-5
 */

import * as fs from 'node:fs';
import * as path from 'node:path';

// ============================================================================
// Types
// ============================================================================

interface FeatureScore {
  feature: number;
  name: string;
  score: number;
  max: number;
}

interface GradeResult {
  app: string;
  language: string;
  llm: string;
  backend: 'spacetime' | 'postgres';
  runId: string;
  date: string;
  promptLevel: number | null;
  totalScore: number;
  maxScore: number;
  percentage: number;
  compiles: boolean;
  runs: boolean;
  locBackend: number | null;
  locFrontend: number | null;
  numFiles: number | null;
  featureScores: FeatureScore[];
}

interface BackendStats {
  runs: number;
  avgPercent: number;
  best: number;
  worst: number;
  totalScore: number;
  maxScore: number;
}

interface LlmStats {
  spacetime?: BackendStats;
  postgres?: BackendStats;
}

interface AppStats {
  spacetime?: BackendStats;
  postgres?: BackendStats;
}

interface Summary {
  byBackend: {
    spacetime?: BackendStats;
    postgres?: BackendStats;
  };
  byLlm: Record<string, LlmStats>;
  byApp: Record<string, AppStats>;
}

interface GradesJson {
  generated: string;
  summary: Summary;
  runs: GradeResult[];
}

// ============================================================================
// File Discovery
// ============================================================================

function findGradingFiles(baseDir: string): string[] {
  const results: string[] = [];

  function walk(dir: string) {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(fullPath);
      } else if (entry.name === 'GRADING_RESULTS.md') {
        results.push(fullPath);
      }
    }
  }

  walk(baseDir);
  return results;
}

// ============================================================================
// Parsing
// ============================================================================

function parsePathInfo(
  filePath: string,
  appsDir: string
): {
  app: string;
  language: string;
  llm: string;
  backend: 'spacetime' | 'postgres';
  runId: string;
} | null {
  // Expected: apps/{app}/{language}/{llm}/{backend}/{runId}/GRADING_RESULTS.md
  const relative = path.relative(appsDir, filePath);
  const parts = relative.split(path.sep);

  if (parts.length < 6) {
    console.warn(`Unexpected path structure: ${filePath}`);
    return null;
  }

  const [app, language, llm, backend, runId] = parts;

  if (backend !== 'spacetime' && backend !== 'postgres') {
    console.warn(`Unknown backend "${backend}" in: ${filePath}`);
    return null;
  }

  return { app, language, llm, backend, runId };
}

function parseGradingFile(
  filePath: string,
  appsDir: string
): GradeResult | null {
  const pathInfo = parsePathInfo(filePath, appsDir);
  if (!pathInfo) return null;

  const content = fs.readFileSync(filePath, 'utf-8');

  // Extract total score: | **Total Feature Score** | 36 / 36 |
  const scoreMatch = content.match(
    /\*\*Total Feature Score\*\*\s*\|\s*(\d+(?:\.\d+)?)\s*\/\s*(\d+)/
  );
  if (!scoreMatch) {
    console.warn(`Could not parse total score from: ${filePath}`);
    return null;
  }

  const totalScore = parseFloat(scoreMatch[1]);
  const maxScore = parseFloat(scoreMatch[2]);
  const percentage = maxScore > 0 ? (totalScore / maxScore) * 100 : 0;

  // Extract date: **Date:** 2026-01-05
  const dateMatch = content.match(/\*\*Date:\*\*\s*(\d{4}-\d{2}-\d{2})/);
  const date = dateMatch ? dateMatch[1] : '';

  // Extract prompt level: | **Prompt Level Used** | 9 (...) |
  const levelMatch = content.match(/\*\*Prompt Level Used\*\*\s*\|\s*(\d+)/);
  const promptLevel = levelMatch ? parseInt(levelMatch[1], 10) : null;

  // Extract compile/run status
  const compiles = /- \[x\] Compiles/i.test(content);
  const runs = /- \[x\] Runs/i.test(content);

  // Extract LOC: | Lines of code (backend) | ~650 |
  const locBackendMatch = content.match(
    /Lines of code \(backend\)\s*\|\s*~?(\d+)/
  );
  const locFrontendMatch = content.match(
    /Lines of code \(frontend\)\s*\|\s*~?(\d+)/
  );
  const numFilesMatch = content.match(
    /Number of files(?:\s+created)?\s*\|\s*(\d+)/
  );

  const locBackend = locBackendMatch ? parseInt(locBackendMatch[1], 10) : null;
  const locFrontend = locFrontendMatch
    ? parseInt(locFrontendMatch[1], 10)
    : null;
  const numFiles = numFilesMatch ? parseInt(numFilesMatch[1], 10) : null;

  // Extract per-feature scores: ## Feature 1: Basic Chat Features (Score: 3 / 3)
  const featureScores: FeatureScore[] = [];
  const featureRegex =
    /## Feature (\d+):\s*([^(]+?)\s*\(Score:\s*(\d+(?:\.\d+)?)\s*\/\s*(\d+)\)/g;
  let match;
  while ((match = featureRegex.exec(content)) !== null) {
    featureScores.push({
      feature: parseInt(match[1], 10),
      name: match[2].trim(),
      score: parseFloat(match[3]),
      max: parseFloat(match[4]),
    });
  }

  return {
    ...pathInfo,
    date,
    promptLevel,
    totalScore,
    maxScore,
    percentage,
    compiles,
    runs,
    locBackend,
    locFrontend,
    numFiles,
    featureScores,
  };
}

// ============================================================================
// Aggregation
// ============================================================================

function calculateStats(results: GradeResult[]): BackendStats | undefined {
  if (results.length === 0) return undefined;

  const percentages = results.map(r => r.percentage);
  const avgPercent =
    percentages.reduce((a, b) => a + b, 0) / percentages.length;
  const best = Math.max(...percentages);
  const worst = Math.min(...percentages);
  const totalScore = results.reduce((a, r) => a + r.totalScore, 0);
  const maxScore = results.reduce((a, r) => a + r.maxScore, 0);

  return {
    runs: results.length,
    avgPercent: Math.round(avgPercent * 10) / 10,
    best: Math.round(best * 10) / 10,
    worst: Math.round(worst * 10) / 10,
    totalScore,
    maxScore,
  };
}

function aggregateResults(results: GradeResult[]): Summary {
  // By backend
  const spacetimeResults = results.filter(r => r.backend === 'spacetime');
  const postgresResults = results.filter(r => r.backend === 'postgres');

  const byBackend: Summary['byBackend'] = {};
  if (spacetimeResults.length > 0)
    byBackend.spacetime = calculateStats(spacetimeResults);
  if (postgresResults.length > 0)
    byBackend.postgres = calculateStats(postgresResults);

  // By LLM
  const llms = [...new Set(results.map(r => r.llm))].sort();
  const byLlm: Record<string, LlmStats> = {};
  for (const llm of llms) {
    const llmResults = results.filter(r => r.llm === llm);
    const llmSpacetime = llmResults.filter(r => r.backend === 'spacetime');
    const llmPostgres = llmResults.filter(r => r.backend === 'postgres');
    byLlm[llm] = {};
    if (llmSpacetime.length > 0)
      byLlm[llm].spacetime = calculateStats(llmSpacetime);
    if (llmPostgres.length > 0)
      byLlm[llm].postgres = calculateStats(llmPostgres);
  }

  // By App
  const apps = [...new Set(results.map(r => r.app))].sort();
  const byApp: Record<string, AppStats> = {};
  for (const app of apps) {
    const appResults = results.filter(r => r.app === app);
    const appSpacetime = appResults.filter(r => r.backend === 'spacetime');
    const appPostgres = appResults.filter(r => r.backend === 'postgres');
    byApp[app] = {};
    if (appSpacetime.length > 0)
      byApp[app].spacetime = calculateStats(appSpacetime);
    if (appPostgres.length > 0)
      byApp[app].postgres = calculateStats(appPostgres);
  }

  return { byBackend, byLlm, byApp };
}

// ============================================================================
// Markdown Generation
// ============================================================================

function generateExecutiveSummary(
  summary: Summary,
  results: GradeResult[]
): string {
  const lines: string[] = [];

  lines.push('# LLM One-Shot Benchmark Summary');
  lines.push('');
  lines.push(`**Generated:** ${new Date().toISOString().split('T')[0]}`);
  lines.push(`**Total Runs:** ${results.length}`);
  lines.push('');

  // Overall by backend
  lines.push('## Overall Results by Backend');
  lines.push('');
  lines.push('| Backend | Runs | Avg Score | Best | Worst |');
  lines.push('|---------|------|-----------|------|-------|');

  if (summary.byBackend.spacetime) {
    const s = summary.byBackend.spacetime;
    lines.push(
      `| SpacetimeDB | ${s.runs} | ${s.avgPercent.toFixed(1)}% | ${s.best.toFixed(1)}% | ${s.worst.toFixed(1)}% |`
    );
  }
  if (summary.byBackend.postgres) {
    const p = summary.byBackend.postgres;
    lines.push(
      `| PostgreSQL | ${p.runs} | ${p.avgPercent.toFixed(1)}% | ${p.best.toFixed(1)}% | ${p.worst.toFixed(1)}% |`
    );
  }

  // Delta
  if (summary.byBackend.spacetime && summary.byBackend.postgres) {
    const delta =
      summary.byBackend.spacetime.avgPercent -
      summary.byBackend.postgres.avgPercent;
    lines.push('');
    lines.push(
      `**SpacetimeDB advantage:** +${delta.toFixed(1)} percentage points`
    );
  }

  lines.push('');

  // By LLM
  lines.push('## Results by LLM');
  lines.push('');
  lines.push('| LLM | STDB Runs | STDB Avg | PG Runs | PG Avg | Delta |');
  lines.push('|-----|-----------|----------|---------|--------|-------|');

  for (const [llm, stats] of Object.entries(summary.byLlm)) {
    const stdb = stats.spacetime;
    const pg = stats.postgres;
    const stdbRuns = stdb ? stdb.runs.toString() : '-';
    const stdbAvg = stdb ? `${stdb.avgPercent.toFixed(1)}%` : '-';
    const pgRuns = pg ? pg.runs.toString() : '-';
    const pgAvg = pg ? `${pg.avgPercent.toFixed(1)}%` : '-';
    let delta = '-';
    if (stdb && pg) {
      const d = stdb.avgPercent - pg.avgPercent;
      delta = d >= 0 ? `+${d.toFixed(1)}%` : `${d.toFixed(1)}%`;
    }
    lines.push(
      `| ${llm} | ${stdbRuns} | ${stdbAvg} | ${pgRuns} | ${pgAvg} | ${delta} |`
    );
  }

  lines.push('');

  // By App
  lines.push('## Results by App');
  lines.push('');
  lines.push('| App | STDB Runs | STDB Avg | PG Runs | PG Avg | Delta |');
  lines.push('|-----|-----------|----------|---------|--------|-------|');

  for (const [app, stats] of Object.entries(summary.byApp)) {
    const stdb = stats.spacetime;
    const pg = stats.postgres;
    const stdbRuns = stdb ? stdb.runs.toString() : '-';
    const stdbAvg = stdb ? `${stdb.avgPercent.toFixed(1)}%` : '-';
    const pgRuns = pg ? pg.runs.toString() : '-';
    const pgAvg = pg ? `${pg.avgPercent.toFixed(1)}%` : '-';
    let delta = '-';
    if (stdb && pg) {
      const d = stdb.avgPercent - pg.avgPercent;
      delta = d >= 0 ? `+${d.toFixed(1)}%` : `${d.toFixed(1)}%`;
    }
    lines.push(
      `| ${app} | ${stdbRuns} | ${stdbAvg} | ${pgRuns} | ${pgAvg} | ${delta} |`
    );
  }

  lines.push('');

  // Individual runs table
  lines.push('## All Runs');
  lines.push('');
  lines.push('| App | LLM | Backend | Date | Score | % |');
  lines.push('|-----|-----|---------|------|-------|---|');

  const sortedResults = [...results].sort((a, b) => {
    if (a.app !== b.app) return a.app.localeCompare(b.app);
    if (a.llm !== b.llm) return a.llm.localeCompare(b.llm);
    if (a.backend !== b.backend) return a.backend.localeCompare(b.backend);
    return b.date.localeCompare(a.date);
  });

  for (const r of sortedResults) {
    const backend = r.backend === 'spacetime' ? 'STDB' : 'PG';
    lines.push(
      `| ${r.app} | ${r.llm} | ${backend} | ${r.date} | ${r.totalScore}/${r.maxScore} | ${r.percentage.toFixed(1)}% |`
    );
  }

  lines.push('');

  return lines.join('\n');
}

function generateAppSummary(app: string, results: GradeResult[]): string {
  const lines: string[] = [];
  const appResults = results.filter(r => r.app === app);

  lines.push(`# ${app} Benchmark Summary`);
  lines.push('');
  lines.push(`**Generated:** ${new Date().toISOString().split('T')[0]}`);
  lines.push(`**Total Runs:** ${appResults.length}`);
  lines.push('');

  // Stats by backend
  const spacetimeResults = appResults.filter(r => r.backend === 'spacetime');
  const postgresResults = appResults.filter(r => r.backend === 'postgres');

  lines.push('## Results by Backend');
  lines.push('');
  lines.push('| Backend | Runs | Avg Score | Best | Worst |');
  lines.push('|---------|------|-----------|------|-------|');

  const stdbStats = calculateStats(spacetimeResults);
  const pgStats = calculateStats(postgresResults);

  if (stdbStats) {
    lines.push(
      `| SpacetimeDB | ${stdbStats.runs} | ${stdbStats.avgPercent.toFixed(1)}% | ${stdbStats.best.toFixed(1)}% | ${stdbStats.worst.toFixed(1)}% |`
    );
  }
  if (pgStats) {
    lines.push(
      `| PostgreSQL | ${pgStats.runs} | ${pgStats.avgPercent.toFixed(1)}% | ${pgStats.best.toFixed(1)}% | ${pgStats.worst.toFixed(1)}% |`
    );
  }

  lines.push('');

  // By LLM
  const llms = [...new Set(appResults.map(r => r.llm))].sort();

  lines.push('## Results by LLM');
  lines.push('');
  lines.push('| LLM | STDB Runs | STDB Avg | PG Runs | PG Avg | Delta |');
  lines.push('|-----|-----------|----------|---------|--------|-------|');

  for (const llm of llms) {
    const llmResults = appResults.filter(r => r.llm === llm);
    const llmStdb = calculateStats(
      llmResults.filter(r => r.backend === 'spacetime')
    );
    const llmPg = calculateStats(
      llmResults.filter(r => r.backend === 'postgres')
    );

    const stdbRuns = llmStdb ? llmStdb.runs.toString() : '-';
    const stdbAvg = llmStdb ? `${llmStdb.avgPercent.toFixed(1)}%` : '-';
    const pgRuns = llmPg ? llmPg.runs.toString() : '-';
    const pgAvg = llmPg ? `${llmPg.avgPercent.toFixed(1)}%` : '-';
    let delta = '-';
    if (llmStdb && llmPg) {
      const d = llmStdb.avgPercent - llmPg.avgPercent;
      delta = d >= 0 ? `+${d.toFixed(1)}%` : `${d.toFixed(1)}%`;
    }
    lines.push(
      `| ${llm} | ${stdbRuns} | ${stdbAvg} | ${pgRuns} | ${pgAvg} | ${delta} |`
    );
  }

  lines.push('');

  // Feature breakdown (aggregate across all runs at the same prompt level)
  const featureMap = new Map<
    number,
    { name: string; stdbScores: number[]; pgScores: number[]; max: number }
  >();

  for (const r of appResults) {
    for (const f of r.featureScores) {
      if (!featureMap.has(f.feature)) {
        featureMap.set(f.feature, {
          name: f.name,
          stdbScores: [],
          pgScores: [],
          max: f.max,
        });
      }
      const entry = featureMap.get(f.feature)!;
      if (r.backend === 'spacetime') {
        entry.stdbScores.push(f.score);
      } else {
        entry.pgScores.push(f.score);
      }
    }
  }

  if (featureMap.size > 0) {
    lines.push('## Feature Scores (Average)');
    lines.push('');
    lines.push('| Feature | Max | STDB Avg | PG Avg | Winner |');
    lines.push('|---------|-----|----------|--------|--------|');

    const sortedFeatures = [...featureMap.entries()].sort(
      (a, b) => a[0] - b[0]
    );
    for (const [num, data] of sortedFeatures) {
      const stdbAvg =
        data.stdbScores.length > 0
          ? (
              data.stdbScores.reduce((a, b) => a + b, 0) /
              data.stdbScores.length
            ).toFixed(2)
          : '-';
      const pgAvg =
        data.pgScores.length > 0
          ? (
              data.pgScores.reduce((a, b) => a + b, 0) / data.pgScores.length
            ).toFixed(2)
          : '-';

      let winner = '-';
      if (data.stdbScores.length > 0 && data.pgScores.length > 0) {
        const stdbNum =
          data.stdbScores.reduce((a, b) => a + b, 0) / data.stdbScores.length;
        const pgNum =
          data.pgScores.reduce((a, b) => a + b, 0) / data.pgScores.length;
        if (stdbNum > pgNum) winner = 'STDB';
        else if (pgNum > stdbNum) winner = 'PG';
        else winner = 'Tie';
      }

      lines.push(
        `| ${num}. ${data.name} | ${data.max} | ${stdbAvg} | ${pgAvg} | ${winner} |`
      );
    }

    lines.push('');
  }

  // Individual runs
  lines.push('## All Runs');
  lines.push('');
  lines.push('| LLM | Backend | Date | Score | % | Level |');
  lines.push('|-----|---------|------|-------|---|-------|');

  const sorted = [...appResults].sort((a, b) => {
    if (a.llm !== b.llm) return a.llm.localeCompare(b.llm);
    if (a.backend !== b.backend) return a.backend.localeCompare(b.backend);
    return b.date.localeCompare(a.date);
  });

  for (const r of sorted) {
    const backend = r.backend === 'spacetime' ? 'STDB' : 'PG';
    const level = r.promptLevel ?? '-';
    lines.push(
      `| ${r.llm} | ${backend} | ${r.date} | ${r.totalScore}/${r.maxScore} | ${r.percentage.toFixed(1)}% | ${level} |`
    );
  }

  lines.push('');

  return lines.join('\n');
}

// ============================================================================
// Main
// ============================================================================

function parseArgs(): { app?: string; llm?: string } {
  const args = process.argv.slice(2);
  const result: { app?: string; llm?: string } = {};

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--app' && args[i + 1]) {
      result.app = args[++i];
    } else if (args[i] === '--llm' && args[i + 1]) {
      result.llm = args[++i];
    }
  }

  return result;
}

function main() {
  const scriptDir = path.dirname(new URL(import.meta.url).pathname);
  // Handle Windows paths (remove leading slash if present)
  const normalizedScriptDir = scriptDir.replace(/^\/([A-Za-z]:)/, '$1');
  const baseDir = path.resolve(normalizedScriptDir, '..');
  const appsDir = path.join(baseDir, 'apps');

  // Output directory: docs/llms/oneshots (relative to repo root)
  const repoRoot = path.resolve(baseDir, '../..');
  const outputDir = path.join(repoRoot, 'docs', 'llms', 'oneshots');

  console.log(`Scanning for grading files in: ${appsDir}`);
  console.log(`Output directory: ${outputDir}`);

  const gradingFiles = findGradingFiles(appsDir);
  console.log(`Found ${gradingFiles.length} grading files`);

  // Parse all files
  let results: GradeResult[] = [];
  for (const file of gradingFiles) {
    const result = parseGradingFile(file, appsDir);
    if (result) {
      results.push(result);
    }
  }

  console.log(`Successfully parsed ${results.length} grading files`);

  // Apply filters
  const filters = parseArgs();
  if (filters.app) {
    results = results.filter(r => r.app === filters.app);
    console.log(`Filtered to app "${filters.app}": ${results.length} results`);
  }
  if (filters.llm) {
    results = results.filter(r => r.llm === filters.llm);
    console.log(`Filtered to LLM "${filters.llm}": ${results.length} results`);
  }

  if (results.length === 0) {
    console.log('No results to summarize.');
    return;
  }

  // Ensure output directory exists
  fs.mkdirSync(outputDir, { recursive: true });

  // Aggregate
  const summary = aggregateResults(results);

  // Generate JSON
  const gradesJson: GradesJson = {
    generated: new Date().toISOString(),
    summary,
    runs: results,
  };

  // Write executive summary
  const execSummaryMd = generateExecutiveSummary(summary, results);
  const execSummaryPath = path.join(outputDir, 'GRADE_SUMMARY.md');
  fs.writeFileSync(execSummaryPath, execSummaryMd);
  console.log(`Wrote: ${execSummaryPath}`);

  // Write JSON
  const jsonPath = path.join(outputDir, 'grades.json');
  fs.writeFileSync(jsonPath, JSON.stringify(gradesJson, null, 2));
  console.log(`Wrote: ${jsonPath}`);

  // Write per-app summaries
  const apps = [...new Set(results.map(r => r.app))];
  for (const app of apps) {
    const appOutputDir = path.join(outputDir, app);
    fs.mkdirSync(appOutputDir, { recursive: true });

    // App markdown
    const appSummaryMd = generateAppSummary(app, results);
    const appSummaryPath = path.join(appOutputDir, 'GRADE_SUMMARY.md');
    fs.writeFileSync(appSummaryPath, appSummaryMd);
    console.log(`Wrote: ${appSummaryPath}`);

    // App JSON
    const appResults = results.filter(r => r.app === app);
    const appSummary = aggregateResults(appResults);
    const appJson: GradesJson = {
      generated: new Date().toISOString(),
      summary: appSummary,
      runs: appResults,
    };
    const appJsonPath = path.join(appOutputDir, 'grades.json');
    fs.writeFileSync(appJsonPath, JSON.stringify(appJson, null, 2));
    console.log(`Wrote: ${appJsonPath}`);
  }

  console.log('\nDone!');
}

main();
