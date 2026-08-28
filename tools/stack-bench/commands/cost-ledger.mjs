#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { readRunJson } from '../src/evidence/artifacts.mjs';

const roundUsd = value => Number(value.toFixed(6));

function sessionRows(run) {
  const rows = [];
  for (const level of run.levels ?? []) {
    for (const [kind, sessions] of [
      ['build', level.buildSession ? [level.buildSession] : []],
      ['resume', level.resumeSession ? [level.resumeSession] : []],
      ['repair', level.fixSessions ?? []],
    ]) {
      for (const [index, session] of sessions.entries()) {
        rows.push({
          level: level.level,
          kind,
          index: index + 1,
          sessionCostUsd: session.costUsd,
          costComplete: session.costComplete === true,
          receipts: session.costReceipts ?? [],
        });
      }
    }
  }
  return rows;
}

export function durableCostLedger(run) {
  const rows = sessionRows(run).map(row => {
    const receiptCostUsd = roundUsd(row.receipts.reduce(
      (sum, entry) => sum + (Number.isFinite(entry?.receipt?.costUsd) ? entry.receipt.costUsd : 0), 0));
    const receiptsComplete = (row.receipts.length === 0 && row.sessionCostUsd === 0)
      || (row.receipts.length > 0 && row.receipts.every(entry => entry?.receipt?.complete === true
        && entry.receipt.reconciled === true && entry.receipt.error === null));
    const differenceUsd = roundUsd(row.sessionCostUsd - receiptCostUsd);
    return { ...row, receiptCostUsd, differenceUsd,
      complete: row.costComplete && receiptsComplete && Math.abs(differenceUsd) <= 0.0001 };
  });
  const reportedCostUsd = roundUsd(run.totals?.costUsd ?? 0);
  const receiptCostUsd = roundUsd(rows.reduce((sum, row) => sum + row.receiptCostUsd, 0));
  const differenceUsd = roundUsd(reportedCostUsd - receiptCostUsd);
  return {
    runId: run.id ?? null,
    pricing: run.pricing ?? null,
    reportedCostUsd,
    receiptCostUsd,
    differenceUsd,
    complete: run.totals?.costComplete === true && rows.every(row => row.complete)
      && Math.abs(differenceUsd) <= 0.0001,
    rows,
  };
}

function parseArgs(argv) {
  const value = name => {
    const index = argv.indexOf(name);
    return index < 0 ? null : argv[index + 1];
  };
  const runPath = value('--run');
  const workdir = value('--workdir');
  if ((runPath === null) === (workdir === null)) {
    throw new Error('use exactly one of --run <run.json> or --workdir <run-directory>');
  }
  return runPath ? resolve(runPath) : resolve(workdir, 'run.json');
}

const entrypoint = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : null;
if (import.meta.url === entrypoint) {
  try {
    const ledger = durableCostLedger(readRunJson(parseArgs(process.argv.slice(2))));
    process.stdout.write(`${JSON.stringify(ledger, null, 2)}\n`);
    if (!ledger.complete) process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 2;
  }
}
