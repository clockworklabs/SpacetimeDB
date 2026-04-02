#!/usr/bin/env node

/**
 * Converts Playwright JSON reporter output into GRADING_RESULTS.md
 * matching the format used by the Chrome MCP grading agent.
 *
 * Usage:
 *   node parse-playwright-results.mjs <results.json> <app-dir> <backend>
 */

import fs from 'fs';
import path from 'path';

const resultsFile = process.argv[2];
const appDir = process.argv[3];
const backend = process.argv[4] || 'unknown';

if (!resultsFile || !appDir) {
  console.error('Usage: node parse-playwright-results.mjs <results.json> <app-dir> <backend>');
  process.exit(1);
}

const results = JSON.parse(fs.readFileSync(resultsFile, 'utf-8'));

// Feature name mapping: spec file name → feature number and name
const FEATURES = {
  'feature-01-basic-chat': { num: 1, name: 'Basic Chat' },
  'feature-02-typing-indicators': { num: 2, name: 'Typing Indicators' },
  'feature-03-read-receipts': { num: 3, name: 'Read Receipts' },
  'feature-04-unread-counts': { num: 4, name: 'Unread Counts' },
  'feature-05-scheduled-messages': { num: 5, name: 'Scheduled Messages' },
  'feature-06-ephemeral-messages': { num: 6, name: 'Ephemeral Messages' },
  'feature-07-reactions': { num: 7, name: 'Message Reactions' },
  'feature-08-edit-history': { num: 8, name: 'Message Editing with History' },
  'feature-09-permissions': { num: 9, name: 'Real-Time Permissions' },
  'feature-10-presence': { num: 10, name: 'Rich User Presence' },
  'feature-11-threading': { num: 11, name: 'Message Threading' },
  'feature-12-private-rooms': { num: 12, name: 'Private Rooms & DMs' },
  'feature-13-activity-indicators': { num: 13, name: 'Room Activity Indicators' },
  'feature-14-draft-sync': { num: 14, name: 'Draft Sync' },
  'feature-15-anonymous-migration': { num: 15, name: 'Anonymous to Registered Migration' },
};

// Parse suites → extract test results per feature
const featureResults = {};

function walkSuites(suites) {
  for (const suite of suites) {
    // Match spec file name to feature
    const specFile = suite.file || '';
    const featureKey = Object.keys(FEATURES).find((k) => specFile.includes(k));

    if (featureKey && suite.specs) {
      if (!featureResults[featureKey]) {
        featureResults[featureKey] = { passed: 0, failed: 0, skipped: 0, tests: [] };
      }
      for (const spec of suite.specs) {
        for (const test of spec.tests || []) {
          const status = test.status || test.results?.[0]?.status || 'unknown';
          const testInfo = {
            title: spec.title,
            status,
            duration: test.results?.[0]?.duration || 0,
          };
          featureResults[featureKey].tests.push(testInfo);
          if (status === 'expected' || status === 'passed') {
            featureResults[featureKey].passed++;
          } else if (status === 'skipped') {
            featureResults[featureKey].skipped++;
          } else {
            featureResults[featureKey].failed++;
          }
        }
      }
    }

    if (suite.suites) {
      walkSuites(suite.suites);
    }
  }
}

walkSuites(results.suites || []);

// Calculate scores: 3 points per feature, proportional to pass rate
// Skipped tests don't count toward total (they're unimplemented)
function calcScore(fr) {
  const total = fr.passed + fr.failed;
  if (total === 0) return 0; // all skipped = 0
  const ratio = fr.passed / total;
  if (ratio >= 1.0) return 3;
  if (ratio >= 0.66) return 2;
  if (ratio >= 0.33) return 1;
  return 0;
}

// Generate report
const date = new Date().toISOString().slice(0, 10);
let totalScore = 0;
let totalMax = 0;
const featureLines = [];
const summaryRows = [];

for (const [key, feat] of Object.entries(FEATURES)) {
  const fr = featureResults[key];
  const score = fr ? calcScore(fr) : 0;
  totalScore += score;
  totalMax += 3;

  const testDetails = fr
    ? fr.tests
        .map((t) => {
          const icon = t.status === 'expected' || t.status === 'passed' ? 'x' : ' ';
          return `- [${icon}] ${t.title} (${t.status}, ${t.duration}ms)`;
        })
        .join('\n')
    : '- [ ] No tests ran';

  featureLines.push(`## Feature ${feat.num}: ${feat.name} (Score: ${score} / 3)\n\n${testDetails}\n`);
  const notes = fr
    ? `${fr.passed}/${fr.passed + fr.failed} passed, ${fr.skipped} skipped`
    : 'No tests';
  summaryRows.push(
    `| ${feat.num}. ${feat.name} | 3 | ${score} | ${notes} |`
  );
}

const report = `# Chat App Grading Results

**Model:** Playwright (automated)
**Date:** ${date}
**Backend:** ${backend}
**Grading Method:** Playwright automated tests

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Features Evaluated**  | 1-15                           |
| **Total Feature Score** | ${totalScore} / ${totalMax}    |

---

${featureLines.join('\n---\n\n')}

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
${summaryRows.join('\n')}
| **TOTAL** | **${totalMax}** | **${totalScore}** | |
`;

const outputPath = path.join(appDir, 'GRADING_RESULTS.md');
fs.writeFileSync(outputPath, report);
console.log(`GRADING_RESULTS.md written to: ${outputPath}`);
console.log(`Total score: ${totalScore}/${totalMax}`);
