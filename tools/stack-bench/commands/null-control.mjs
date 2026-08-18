#!/usr/bin/env node
// Grade the real validated production scenarios against a reachable app that
// implements nothing. Every point-bearing criterion must conclusively fail.

import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { readArtifactPayload, writeRunJson } from '../src/evidence/artifacts.mjs';
import { calibrationQualificationIdentity, resolveCalibrationForRelease } from '../src/composition/calibration-compiler.mjs';
import { analyseNullReports } from '../src/evidence/null-control-analysis.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { isDeclaredLevel, listTracks, loadTrack, suitesFor } from '../src/composition/tracks.mjs';
import { controllerRunner } from '../src/runtime/runner-environment.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../src/project-paths.mjs';
const GRADE = join(ROOT, 'grader', 'grade.mjs');

export function parseNullControlArgs(argv) {
  const args = { tracks: listTracks(), level: null, audit: false };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--track') args.tracks = argv[++i].split(',').filter(Boolean);
    else if (argv[i] === '--level') args.level = Number(argv[++i]);
    else if (argv[i] === '--recipe') args.recipe = argv[++i];
    else if (argv[i] === '--out') args.out = argv[++i];
    else if (argv[i] === '--audit') args.audit = true;
    else if (argv[i] === '--parent-attempt-id') args.parentAttemptId = argv[++i];
    else { console.error(`Unknown argument: ${argv[i]}`); process.exit(2); }
  }
  if (args.level !== null && (!Number.isInteger(args.level) || args.level < 1)) {
    throw new Error('--level must be a positive integer');
  }
  if (args.level !== null && args.tracks.length !== 1) {
    throw new Error('--level requires exactly one --track');
  }
  if (args.recipe && args.level === null) throw new Error('--recipe requires --level');
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

export function nullControlSuites(track, selectedLevel = null, binding = null) {
  if (selectedLevel !== null && !isDeclaredLevel(track, selectedLevel)) {
    throw new Error(`L${selectedLevel} is not declared for ${track.name}`);
  }
  if (binding) {
    if (selectedLevel === null) throw new Error('recipe-bound null control requires one level');
    if (!Array.isArray(binding.execution) || !binding.execution.length) {
      throw new Error('recipe-bound null control requires a typed execution plan');
    }
    const executionIds = new Set();
    const mappedKeys = new Set();
    const suites = binding.execution.map(execution => {
      if (executionIds.has(execution.id)) {
        throw new Error(`recipe-bound null control repeats execution ${execution.id}`);
      }
      executionIds.add(execution.id);
      const checks = binding.release.checkCatalog.filter(check => check.executionId === execution.id);
      if (!checks.length) {
        throw new Error(`recipe-bound null control execution ${execution.id} maps no checks`);
      }
      for (const check of checks) {
        if (mappedKeys.has(check.stableKey)) {
          throw new Error(`recipe-bound null control maps check ${check.stableKey} more than once`);
        }
        mappedKeys.add(check.stableKey);
      }
      return { id: execution.id, spec: resolve(track.dir, execution.source),
        level: selectedLevel, checks };
    });
    const missing = binding.release.checkCatalog
      .filter(check => !mappedKeys.has(check.stableKey)).map(check => check.stableKey);
    if (missing.length) {
      throw new Error(`recipe-bound null control leaves checks unmapped: ${missing.join(', ')}`);
    }
    return suites;
  }
  const seen = new Set();
  const suites = [];
  const levels = selectedLevel === null
    ? Array.from({ length: track.validatedThrough }, (_, index) => index + 1)
    : [selectedLevel];
  for (const level of levels) {
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
  const args = parseNullControlArgs(process.argv);
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
  let qualification = null;
  try {
    const port = await listen(server);
    const url = `http://127.0.0.1:${port}`;
    for (const trackName of args.tracks) {
      const track = loadTrack(trackName);
      let binding = null;
      if (args.level !== null) {
        binding = resolveRecipeRelease(track, args.level, args.recipe);
        if (!binding) throw new Error(`${trackName} L${args.level} has no recipe release`);
        const calibration = resolveCalibrationForRelease(binding.release,
          { trackRoot: track.dir, stackBenchRoot: ROOT });
        if (!calibration) throw new Error(`${trackName} L${args.level} has no calibration`);
        qualification = { binding, calibration, identity: calibrationQualificationIdentity(calibration) };
      }
      const selectedSuites = nullControlSuites(track, args.level, binding);
      const resolvedRecipe = binding
        ? `${binding.release.id}@${binding.release.version}` : args.recipe;
      for (const suite of selectedSuites) {
        const reportPath = join(reportsDir, `${trackName}-l${suite.level}-${suite.id.replaceAll('@', '-')}.json`);
        process.stdout.write(`${trackName} L${suite.level} ${suite.id} (${basename(suite.spec)}) ... `);
        await runGrade(['--url', url, '--level', String(suite.level), '--spec', suite.spec,
          '--backend', 'postgres', '--track', trackName, '--app', app, '--out', reportPath,
          '--parent-attempt-id', nullAttemptId,
          ...(resolvedRecipe ? ['--recipe', resolvedRecipe] : []),
          ...(binding ? ['--expected-recipe-sha256', binding.release.contentSha256] : []),
          ...(suite.checks ?? []).flatMap(check => ['--selected-check', check.stableKey])]);
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
      identities: qualification ? {
        recipe: { id: qualification.binding.release.id, version: qualification.binding.release.version,
          sha256: qualification.binding.release.contentSha256, state: qualification.binding.release.state },
        calibration: { ...qualification.identity, state: qualification.calibration.state },
      } : undefined,
      durationMs: Date.now() - started,
      runner: controllerRunner(),
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

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch(error => {
    console.error(error.stack ?? error.message);
    process.exitCode = 2;
  });
}
