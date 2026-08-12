#!/usr/bin/env node
// Grade the real validated production scenarios against a reachable app that
// implements nothing. Every point-bearing criterion must conclusively fail.

import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readArtifactPayload, writeRunJson } from './artifacts.mjs';
import { analyseNullReports } from './null-control-analysis.mjs';
import { listTracks, loadTrack, suitesFor } from './tracks.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const GRADE = join(ROOT, 'grader', 'grade.mjs');

function parseArgs(argv) {
  const args = { tracks: listTracks(), audit: false };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--track') args.tracks = argv[++i].split(',').filter(Boolean);
    else if (argv[i] === '--out') args.out = argv[++i];
    else if (argv[i] === '--audit') args.audit = true;
    else if (argv[i] === '--parent-attempt-id') args.parentAttemptId = argv[++i];
    else { console.error(`Unknown argument: ${argv[i]}`); process.exit(2); }
  }
  return args;
}

function runGrade(argv, timeoutMs = 300_000) {
  return new Promise((resolve, reject) => {
    execFile(process.execPath, [GRADE, ...argv], {
      encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: timeoutMs,
    }, (error, stdout, stderr) => {
      if (error) {
        error.message = `grader failed: ${error.message}\n${stdout}\n${stderr}`;
        reject(error);
      } else resolve({ stdout, stderr });
    });
  });
}

function uniqueValidatedSuites(track) {
  const seen = new Set();
  const suites = [];
  for (let level = 1; level <= track.validatedThrough; level++) {
    for (const suite of suitesFor(track, level)) {
      if (seen.has(suite.spec)) continue;
      seen.add(suite.spec);
      suites.push({ ...suite, level });
    }
  }
  return suites;
}

async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return server.address().port;
}

async function main() {
  const args = parseArgs(process.argv);
  const nullAttemptId = `null-control-${new Date().toISOString().replace(/[:.]/g, '-')}`;
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-null-'));
  const app = join(work, 'app');
  const reportsDir = join(work, 'reports');
  mkdirSync(app, { recursive: true });
  mkdirSync(reportsDir, { recursive: true });

  // Root navigation succeeds, proving the browser and server are healthy. All
  // application/API behavior is absent: non-navigation requests get 404.
  const server = createServer((request, response) => {
    if (request.method === 'GET' && (request.url === '/' || request.headers.accept?.includes('text/html'))) {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end('<!doctype html><html><head><title>Null control</title></head><body></body></html>');
    } else {
      response.writeHead(404, { 'content-type': 'application/json' });
      response.end('{"error":"not implemented"}');
    }
  });

  const started = Date.now();
  const suiteReports = [];
  try {
    const port = await listen(server);
    const url = `http://127.0.0.1:${port}`;
    for (const trackName of args.tracks) {
      const track = loadTrack(trackName);
      for (const suite of uniqueValidatedSuites(track)) {
        const reportPath = join(reportsDir, `${trackName}-l${suite.level}-${suite.id.replaceAll('@', '-')}.json`);
        process.stdout.write(`${trackName} L${suite.level} ${suite.id} (${basename(suite.spec)}) ... `);
        await runGrade(['--url', url, '--level', String(suite.level), '--spec', suite.spec,
          '--backend', 'postgres', '--track', trackName, '--app', app, '--out', reportPath,
          '--parent-attempt-id', nullAttemptId]);
        const report = readArtifactPayload(reportPath, { expectedKind: 'grade' });
        suiteReports.push({ track: trackName, level: suite.level, id: suite.id,
          scenario: relative(track.dir, suite.spec).replaceAll('\\', '/'), report });
        console.log(`${report.total}/${report.max}`);
      }
    }

    const analysis = analyseNullReports(suiteReports);
    const artifact = {
      id: nullAttemptId,
      kind: 'null_control',
      startedAt: new Date(started).toISOString(),
      completedAt: new Date().toISOString(),
      parentAttemptId: args.parentAttemptId ?? null,
      durationMs: Date.now() - started,
      tracks: args.tracks,
      ...analysis,
    };
    const outputPath = resolve(args.out ?? join(ROOT, 'results', `${artifact.id}.json`));
    writeRunJson(outputPath, artifact);
    console.log(JSON.stringify({
      id: artifact.id,
      kind: artifact.kind,
      durationMs: artifact.durationMs,
      tracks: artifact.tracks,
      ok: artifact.ok,
      summary: artifact.summary,
      artifact: outputPath,
    }, null, 2));
    if (!analysis.ok && !args.audit) process.exitCode = 1;
  } finally {
    await new Promise(resolve => server.close(resolve));
    rmSync(work, { recursive: true, force: true });
  }
}

main().catch(error => {
  console.error(error.stack ?? error.message);
  process.exitCode = 2;
});
