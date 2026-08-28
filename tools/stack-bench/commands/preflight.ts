#!/usr/bin/env node

import { parsePreflightArgs, printPreflightReport, runPreflight,
  writePreflightReport } from '../src/runtime/preflight.mjs';

let request;
try {
  request = parsePreflightArgs(process.argv);
} catch (error) {
  console.error(`preflight: ${error instanceof Error ? error.message : String(error)}`);
  console.error('Usage: stack-bench preflight --backend spacetime[,postgres,mongodb] [--track ecommerce] [--levels 1-2] [--smoke]');
  process.exit(2);
}

const report = runPreflight(request);
if (request.report) writePreflightReport(request.report, report);
if (request.json) console.log(JSON.stringify(report, null, 2));
else printPreflightReport(report);
process.exitCode = report.ok ? 0 : 1;
