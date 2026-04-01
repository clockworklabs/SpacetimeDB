#!/usr/bin/env node

/**
 * Parses OpenTelemetry logs from Claude Code sessions
 * and generates a COST_REPORT.md with exact token counts.
 *
 * Usage:
 *   node parse-telemetry.mjs <run-dir>
 *
 * Reads: telemetry/logs.jsonl (OTLP JSON log records)
 * Writes: <run-dir>/COST_REPORT.md
 */

import fs from 'fs';
import path from 'path';

const runDir = process.argv[2];
const endTimeOverride = process.argv[3]; // optional: ISO timestamp for reparsing old runs
if (!runDir) {
  console.error('Usage: node parse-telemetry.mjs <run-dir> [end-time-iso]');
  console.error('  end-time-iso: optional upper bound for time filtering (e.g. "2026-03-30T22:00:00Z")');
  process.exit(1);
}

// telemetry/logs.jsonl is in the parent of the run dir
// e.g. runDir = telemetry/spacetime-level1-20260330/ → parent = telemetry/
const telemetryDir = path.dirname(path.resolve(runDir));
const logsFile = path.join(telemetryDir, 'logs.jsonl');

if (!fs.existsSync(logsFile)) {
  console.error(`Telemetry file not found: ${logsFile}`);
  console.error('Make sure the OTel Collector is running and Claude Code has CLAUDE_CODE_ENABLE_TELEMETRY=1');
  process.exit(1);
}

// Read metadata
const metadataFile = path.join(runDir, 'metadata.json');
const metadata = fs.existsSync(metadataFile)
  ? JSON.parse(fs.readFileSync(metadataFile, 'utf-8'))
  : { level: '?', backend: '?', timestamp: '?' };

// Time-range filtering: only include records from this run's time window
const startTime = metadata.startedAtUtc || metadata.startedAt;
const endTime = endTimeOverride || metadata.endedAtUtc || metadata.endedAt;
const startMs = startTime ? new Date(startTime).getTime() : 0;
const endMs = endTime ? new Date(endTime).getTime() : Date.now();

if (!endTime) {
  console.warn('WARNING: No end time found in metadata — using current time as upper bound.');
  console.warn('         The run may have crashed or the metadata update failed.');
}
console.log(`Filtering telemetry: ${startTime || '(start)'} → ${endTime || '(now)'}`);

// Parse OTLP log records
// The format depends on the collector version, but generally each line is a JSON object
// containing log records with attributes that include token counts.
const lines = fs.readFileSync(logsFile, 'utf-8').trim().split('\n').filter(Boolean);

const apiCalls = [];
let totalInput = 0;
let totalOutput = 0;
let totalCacheRead = 0;
let totalCacheCreation = 0;
let totalCostUsd = 0;

let skippedOutOfRange = 0;
let skippedNonApi = 0;

for (const line of lines) {
  try {
    const record = JSON.parse(line);

    // OTLP log records can be nested in different ways depending on the collector.
    // We look for attributes containing token counts.
    const attrs = extractAttributes(record);

    // Filter by time range — only include records within this run's window
    const eventTimestamp = attrs['event.timestamp'] || attrs.timestamp;
    if (eventTimestamp) {
      const eventMs = new Date(eventTimestamp).getTime();
      if (eventMs < startMs || eventMs > endMs) {
        skippedOutOfRange++;
        continue;
      }
    }

    // Filter by event type — only api_request records have token data
    if (attrs._eventType && attrs._eventType !== 'claude_code.api_request') {
      skippedNonApi++;
      continue;
    }

    if (attrs.input_tokens !== undefined || attrs['input_tokens'] !== undefined) {
      const call = {
        inputTokens: Number(attrs.input_tokens || attrs['input_tokens'] || 0),
        outputTokens: Number(attrs.output_tokens || attrs['output_tokens'] || 0),
        cacheReadTokens: Number(attrs.cache_read_tokens || attrs['cache_read_tokens'] || 0),
        cacheCreationTokens: Number(attrs.cache_creation_tokens || attrs['cache_creation_tokens'] || 0),
        costUsd: Number(attrs.cost_usd || attrs['cost_usd'] || 0),
        model: attrs.model || attrs['model'] || 'unknown',
        durationMs: Number(attrs.duration_ms || attrs['duration_ms'] || 0),
        timestamp: eventTimestamp || record.timeUnixNano || '',
      };

      apiCalls.push(call);
      totalInput += call.inputTokens;
      totalOutput += call.outputTokens;
      totalCacheRead += call.cacheReadTokens;
      totalCacheCreation += call.cacheCreationTokens;
      totalCostUsd += call.costUsd;
    }
  } catch {
    // Skip unparseable lines
  }
}

// Generate report
const totalTokens = totalInput + totalOutput;
const totalDurationSec = apiCalls.reduce((sum, c) => sum + c.durationMs, 0) / 1000;

const report = `# Cost Report

**App:** chat-app
**Backend:** ${metadata.backend}
**Level:** ${metadata.level}
**Date:** ${new Date().toISOString().slice(0, 10)}
**Started:** ${metadata.startedAt || metadata.timestamp}

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | ${totalInput.toLocaleString()} |
| Total output tokens     | ${totalOutput.toLocaleString()} |
| Total tokens            | ${totalTokens.toLocaleString()} |
| Cache read tokens       | ${totalCacheRead.toLocaleString()} |
| Cache creation tokens   | ${totalCacheCreation.toLocaleString()} |
| Total cost (USD)        | $${totalCostUsd.toFixed(4)} |
| Total API time          | ${totalDurationSec.toFixed(1)}s |
| API calls               | ${apiCalls.length} |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
${apiCalls.map((c, i) =>
  `| ${i + 1} | ${c.model} | ${c.inputTokens.toLocaleString()} | ${c.outputTokens.toLocaleString()} | ${c.cacheReadTokens.toLocaleString()} | $${c.costUsd.toFixed(4)} | ${(c.durationMs / 1000).toFixed(1)}s |`
).join('\n')}

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
`;

const reportPath = path.join(runDir, 'COST_REPORT.md');
fs.writeFileSync(reportPath, report);

console.log(`Parsed ${apiCalls.length} API calls from ${lines.length} telemetry records.`);
console.log(`  Skipped: ${skippedOutOfRange} out of time range, ${skippedNonApi} non-API events`);
console.log(`Total tokens: ${totalTokens.toLocaleString()} (${totalInput.toLocaleString()} in / ${totalOutput.toLocaleString()} out)`);
console.log(`Total cost: $${totalCostUsd.toFixed(4)}`);
console.log(`Report saved to: ${reportPath}`);

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Extract attributes from an OTLP log record.
 * The structure varies by collector version and export format.
 */
function extractAttributes(record) {
  const attrs = {};

  // Direct attributes
  if (record.attributes) {
    flattenAttributes(record.attributes, attrs);
  }

  // Nested in resourceLogs → scopeLogs → logRecords
  if (record.resourceLogs) {
    for (const rl of record.resourceLogs) {
      for (const sl of rl.scopeLogs || []) {
        for (const lr of sl.logRecords || []) {
          // Capture event type from body (e.g. "claude_code.api_request")
          if (lr.body?.stringValue) {
            attrs._eventType = lr.body.stringValue;
          }
          if (lr.attributes) {
            flattenAttributes(lr.attributes, attrs);
          }
          if (lr.body?.kvlistValue?.values) {
            flattenAttributes(lr.body.kvlistValue.values, attrs);
          }
        }
      }
    }
  }

  return attrs;
}

function flattenAttributes(attrList, out) {
  if (Array.isArray(attrList)) {
    for (const kv of attrList) {
      if (kv.key && kv.value) {
        out[kv.key] = kv.value.stringValue || kv.value.intValue || kv.value.doubleValue || kv.value.boolValue;
      }
    }
  } else if (typeof attrList === 'object') {
    Object.assign(out, attrList);
  }
}
