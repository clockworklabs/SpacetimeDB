import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeArtifact } from './artifacts.mjs';
import { acquireCampaignLock, releaseCampaignLock } from './campaign-lock.mjs';
import { compileCampaignFile } from './campaign-compiler.mjs';
import { claimNextAttempt, finishCampaignExecution, initializeCampaignDirectory,
  markInterruptedExecution, readCampaignState, writeCampaignState } from './campaign-scheduler.mjs';
import { rescueSupervisedLease, runBounded } from './reference-live.mjs';
import { canonicalDefinitionJson } from './definition-plan.mjs';
import { runPreflight } from './preflight.mjs';
import { sha256 } from './provenance.mjs';
import { validateReleaseManifest } from './release-manifest.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const BENCH = join(ROOT, 'bench.mjs');

function contained(root, path, label) {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is not a child of the campaign directory`);
  }
  return absolute;
}

export function attemptArgv(plan, attempt, output) {
  const levels = `${Math.min(...attempt.levels)}-${Math.max(...attempt.levels)}`;
  const guidanceDocument = attempt.condition?.guidance?.documents?.[attempt.stack];
  if (!guidanceDocument) {
    throw new Error(`attempt ${attempt.id} has no guidance document for ${attempt.stack}`);
  }
  const args = [BENCH,
    '--backend', attempt.stack,
    '--track', plan.definition.track,
    '--levels', levels,
    '--run-index', '0',
    '--out', output,
    '--agent-adapter', attempt.agentAdapter,
    '--model', attempt.model,
    '--guidance', attempt.guidance,
    '--guidance-document-json', JSON.stringify(guidanceDocument),
    '--condition-json', JSON.stringify(attempt.condition),
    '--fix-rounds', String(plan.definition.budgets.fixRounds),
    '--parent-attempt-id', attempt.id,
    '--no-media'];
  for (const pack of plan.definition.selection.packs) args.push('--pack', pack);
  for (const check of plan.definition.selection.checks) args.push('--check', check);
  args.push('--skills-json', JSON.stringify(attempt.skills));
  if (plan.definition.budgets.maxCostUsdPerAttempt !== null) {
    args.push('--max-budget-usd', String(plan.definition.budgets.maxCostUsdPerAttempt));
  }
  return args;
}

export function validateCampaignRun(plan, attempt, run, { buildImage = null } = {}) {
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter
    && item.model === attempt.model);
  const condition = plan.conditions.find(item => item.sha256 === attempt.condition?.sha256);
  const expectedLevels = [...attempt.levels].sort((a, b) => a - b);
  const actualLevels = (run.levels ?? []).map(level => level.level).sort((a, b) => a - b);
  const exactLevels = canonicalDefinitionJson(actualLevels) === canonicalDefinitionJson(expectedLevels);
  const interruptedPrefix = actualLevels.length < expectedLevels.length
    && actualLevels.every((level, index) => level === expectedLevels[index])
    && ['harness_failure', 'ungraded'].includes(run.outcome?.kind);
  const mismatches = [];
  const mismatch = (condition, field) => { if (condition) mismatches.push(field); };
  mismatch(run.artifactEnvelope?.attempt?.parentId !== attempt.id, 'attempt.parentId');
  mismatch(run.track !== plan.definition.track, 'track');
  mismatch(run.backend !== attempt.stack, 'backend');
  mismatch(run.model !== attempt.model, 'model');
  mismatch(run.guidance !== attempt.guidance, 'guidance');
  mismatch(!condition || canonicalDefinitionJson(run.condition)
    !== canonicalDefinitionJson(attempt.condition), 'condition');
  mismatch(canonicalDefinitionJson(run.selectionRequest)
    !== canonicalDefinitionJson(plan.definition.selection), 'selectionRequest');
  mismatch(canonicalDefinitionJson(run.skills) !== canonicalDefinitionJson(attempt.skills), 'skills');
  mismatch(!exactLevels && !interruptedPrefix, 'levels');
  mismatch(run.artifactEnvelope?.identities?.agentAdapter?.sha256 !== agent?.identity.sha256,
    'identities.agentAdapter.sha256');
  mismatch(run.artifactEnvelope?.identities?.engine?.sha256 !== plan.identities.engine.sha256,
    'identities.engine.sha256');
  mismatch(run.artifactEnvelope?.identities?.stackAdapter?.id !== attempt.stack,
    'identities.stackAdapter.id');
  mismatch(run.artifactEnvelope?.identities?.stackAdapter?.version
    !== plan.stacks.find(item => item.id === attempt.stack)?.version,
    'identities.stackAdapter.version');
  mismatch(plan.definition.runtime.buildImage !== null
    && run.runtime?.buildImage !== plan.definition.runtime.buildImage, 'runtime.buildImage');
  mismatch(plan.definition.runtime.buildImage === null && buildImage !== null
    && run.runtime?.buildImage !== buildImage, 'runtime.buildImage');
  if (exactLevels && ['passed', 'app_failure'].includes(run.outcome?.kind)) {
    const levelOutcomes = (run.levels ?? []).map(level => level.outcome?.kind);
    mismatch(run.outcome.kind === 'passed' && levelOutcomes.some(kind => kind !== 'passed'),
      'outcome.kind');
    mismatch(run.outcome.kind === 'app_failure'
      && !levelOutcomes.some(kind => kind === 'app_failure'), 'outcome.kind');
    for (const level of run.levels ?? []) {
      const repair = level.repair;
      const at = `levels.L${level.level}.repair`;
      const validObject = repair && typeof repair === 'object' && !Array.isArray(repair);
      mismatch(!validObject, at);
      if (!validObject) continue;
      mismatch(!Number.isInteger(level.fixRounds) || level.fixRounds < 0
        || level.fixRounds > plan.definition.budgets.fixRounds, `levels.L${level.level}.fixRounds`);
      mismatch(repair.budgetRounds !== plan.definition.budgets.fixRounds, `${at}.budgetRounds`);
      mismatch(repair.roundsUsed !== level.fixRounds, `${at}.roundsUsed`);
      const validScore = Number.isInteger(level.score) && Number.isInteger(level.max)
        && level.max > 0 && level.score >= 0 && level.score <= level.max;
      mismatch(!validScore, `levels.L${level.level}.score`);
      if (level.outcome?.kind === 'passed') {
        const expected = level.fixRounds > 0 ? 'corrected' : 'not-needed';
        mismatch(repair.status !== expected, `${at}.status`);
        mismatch(validScore && level.score !== level.max, `levels.L${level.level}.score`);
      } else if (level.outcome?.kind === 'app_failure') {
        mismatch(repair.status !== 'budget-exhausted', `${at}.status`);
        mismatch(repair.roundsUsed !== repair.budgetRounds, `${at}.roundsUsed`);
        // The level score covers only the newly requested criteria. A level can
        // earn every one of those points and still have a legitimate
        // application failure because an inherited guarantee regressed or a
        // deliberately zero-point diagnostic failed. classifyBundle records
        // both cases in appFailures, so do not turn that evidence into a
        // harness failure merely because the requested score is perfect.
        const inheritedDeficit = Number.isInteger(level.regression?.score)
          && Number.isInteger(level.regression?.max)
          && level.regression.max > 0
          && level.regression.score < level.regression.max;
        const recordedFailure = Array.isArray(level.outcome?.appFailures)
          && level.outcome.appFailures.length > 0;
        mismatch(validScore && level.score === level.max
          && !inheritedDeficit && !recordedFailure, `levels.L${level.level}.score`);
      } else {
        mismatch(true, `${at}.levelOutcome`);
      }
    }
  }
  if (plan.definition.budgets.maxCostUsdPerAttempt !== null) {
    const cost = run.totals?.costUsd;
    const missingAllowed = interruptedPrefix && actualLevels.length === 0 && cost == null;
    mismatch(!missingAllowed && (!Number.isFinite(cost)
      || cost > plan.definition.budgets.maxCostUsdPerAttempt), 'totals.costUsd');
  }
  if (mismatches.length) {
    throw new Error(`run.json does not match its planned campaign attempt: ${mismatches.join(', ')}`);
  }
  return run;
}

function readAttemptResult(plan, attempt, output, processResult) {
  const runPath = join(output, 'run.json');
  let run = null;
  let artifactError = null;
  if (existsSync(runPath)) {
    try {
      run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
      validateCampaignRun(plan, attempt, run, { buildImage: processResult.buildImage });
    }
    catch (error) { artifactError = error; }
  }
  if (artifactError) return { exitCode: processResult.code, timedOut: processResult.timedOut,
    run: { outcome: { kind: 'harness_failure',
      reason: `run.json is invalid: ${artifactError.message}` } } };
  if (!run && processResult.code !== 0 && !processResult.timedOut) {
    const detail = (processResult.stderrTail || processResult.stdoutTail || processResult.error?.message || '')
      .split(/\r?\n/).map(line => line.trim()).filter(Boolean).slice(-4).join(' | ').slice(0, 800);
    return { exitCode: processResult.code, timedOut: false, run: { outcome: {
      kind: 'harness_failure', reason: detail || 'attempt ended before producing run.json' } } };
  }
  return { exitCode: processResult.code, timedOut: processResult.timedOut, run };
}

export function verifyCampaignRuntime(plan, env = process.env) {
  if (plan.state !== 'frozen') return structuredClone(plan.definition.runtime);
  const expected = plan.definition.runtime;
  if (env.STACK_BENCH_CONTROLLER_IMAGE !== expected.controllerImage) {
    throw new Error('running controller image does not match the frozen campaign');
  }
  if (expected.releaseManifestSha256 === null) return structuredClone(expected);
  if (typeof env.STACK_BENCH_RELEASE_MANIFEST !== 'string'
    || env.STACK_BENCH_RELEASE_MANIFEST.trim() === '') {
    throw new Error('STACK_BENCH_RELEASE_MANIFEST is required for a frozen campaign');
  }
  let bytes;
  try { bytes = readFileSync(resolve(env.STACK_BENCH_RELEASE_MANIFEST)); }
  catch (error) {
    throw new Error(`cannot read frozen campaign release manifest: ${error.message}`, { cause: error });
  }
  if (sha256(bytes) !== expected.releaseManifestSha256) {
    throw new Error('release manifest does not match the frozen campaign');
  }
  let manifest;
  try { manifest = validateReleaseManifest(JSON.parse(bytes.toString('utf8'))); }
  catch (error) {
    throw new Error(`frozen campaign release manifest is invalid: ${error.message}`, { cause: error });
  }
  const controller = manifest.images.find(image => image.role === 'controller');
  const build = manifest.images.find(image => image.role === 'build-sandbox');
  if (controller?.reference !== expected.controllerImage
    || build?.reference !== expected.buildImage
    || controller?.platform !== expected.platform
    || build?.platform !== expected.platform) {
    throw new Error('release manifest images do not match the frozen campaign');
  }
  return structuredClone(expected);
}

export function campaignExecutionEnvironment(plan, env = process.env) {
  const executionEnv = { ...env };
  if (plan.definition.runtime.buildImage !== null) {
    if (executionEnv.STACK_BENCH_IMAGE
      && executionEnv.STACK_BENCH_IMAGE !== plan.definition.runtime.buildImage) {
      throw new Error('ambient STACK_BENCH_IMAGE conflicts with the campaign build image');
    }
    executionEnv.STACK_BENCH_IMAGE = plan.definition.runtime.buildImage;
  }
  verifyCampaignRuntime(plan, executionEnv);
  return executionEnv;
}

function validateCampaignAdmission(input, plan, directory) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    throw new Error('campaign admission payload must be an object');
  }
  const fields = new Set(['schemaVersion', 'campaignId', 'campaignSha256', 'createdAt',
    'ok', 'runtime', 'agents', 'conditions', 'reports']);
  for (const key of Object.keys(input)) if (!fields.has(key)) throw new Error(`campaign admission.${key} is unknown`);
  if (input.schemaVersion !== 1 || input.campaignId !== plan.id
    || input.campaignSha256 !== plan.contentSha256
    || typeof input.createdAt !== 'string' || Number.isNaN(Date.parse(input.createdAt))
    || typeof input.ok !== 'boolean') throw new Error('campaign admission identity or metadata is invalid');
  if (canonicalDefinitionJson(input.runtime) !== canonicalDefinitionJson(plan.definition.runtime)) {
    throw new Error('campaign admission runtime does not match the compiled plan');
  }
  const expectedAgents = plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model,
    identity: agent.identity }));
  if (canonicalDefinitionJson(input.agents) !== canonicalDefinitionJson(expectedAgents)) {
    throw new Error('campaign admission agents do not match the compiled plan');
  }
  if (canonicalDefinitionJson(input.conditions) !== canonicalDefinitionJson(plan.conditions)) {
    throw new Error('campaign admission conditions do not match the compiled plan');
  }
  if (!Array.isArray(input.reports)) throw new Error('campaign admission reports must be an array');
  const adapters = [...new Set(plan.agents.map(agent => agent.adapter))].sort();
  if (input.reports.length !== adapters.length) throw new Error('campaign admission reports are incomplete');
  const expectedBackends = plan.stacks.map(stack => stack.id);
  const expectedResultsDir = resolve(directory);
  for (const adapter of adapters) {
    const matches = input.reports.filter(report => report?.request?.agentAdapter === adapter);
    if (matches.length !== 1) throw new Error(`campaign admission must contain one ${adapter} report`);
    const report = matches[0];
    if (report.schemaVersion !== 1 || typeof report.ok !== 'boolean' || !Array.isArray(report.checks)
      || !report.summary || typeof report.summary !== 'object'
      || !report.checks.every(check => check && typeof check === 'object'
        && typeof check.id === 'string' && ['pass', 'warn', 'fail'].includes(check.status)
        && typeof check.summary === 'string')
      || report.summary.passed !== report.checks.filter(check => check.status === 'pass').length
      || report.summary.failed !== report.checks.filter(check => check.status === 'fail').length
      || report.summary.warnings !== report.checks.filter(check => check.status === 'warn').length
      || report.ok !== !report.checks.some(check => check.status === 'fail')) {
      throw new Error(`campaign admission report for ${adapter} is malformed`);
    }
    const request = report.request;
    if (canonicalDefinitionJson(request.backends) !== canonicalDefinitionJson(expectedBackends)
      || request.track !== plan.definition.track
      || canonicalDefinitionJson(request.levels) !== canonicalDefinitionJson(plan.definition.levels)
      || request.runIndex !== 0
      || canonicalDefinitionJson(request.packs) !== canonicalDefinitionJson(plan.definition.selection.packs)
      || canonicalDefinitionJson(request.checks) !== canonicalDefinitionJson(plan.definition.selection.checks)
      || request.smoke !== true
      || (plan.definition.runtime.buildImage !== null
        && request.image !== plan.definition.runtime.buildImage)
      || resolve(request.resultsDir) !== expectedResultsDir) {
      throw new Error(`campaign admission report for ${adapter} does not match the compiled scope`);
    }
  }
  if (input.ok !== input.reports.every(report => report.ok)) {
    throw new Error('campaign admission verdict does not match its reports');
  }
  return structuredClone(input);
}

function readCampaignAdmission(directory, id, plan) {
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  const artifact = readArtifact(path, { expectedKind: 'campaign_admission', expectedId: id });
  if (artifact.identities.experiment?.sha256 !== plan.contentSha256) {
    throw new Error(`campaign admission ${id} has the wrong experiment identity`);
  }
  return validateCampaignAdmission(artifact.payload, plan, directory);
}

function assertAdmissionReferences(plan, directory, state) {
  const ids = [...new Set(state.attempts.flatMap(attempt =>
    attempt.executions.map(execution => execution.admissionId)))];
  for (const id of ids) {
    const admission = readCampaignAdmission(directory, id, plan);
    if (!admission.ok) throw new Error(`campaign execution references failed admission ${id}`);
  }
  return state;
}

export function inspectCampaign(directory) {
  const current = readCampaignState(directory);
  return { ...current,
    state: assertAdmissionReferences(current.plan, current.paths.root, current.state) };
}

function publicRecoveryProvesCleanup(output, backend) {
  const path = join(output, 'recovery.json');
  if (!existsSync(path)) return false;
  const recovery = readArtifactPayload(path, { expectedKind: 'recovery' });
  return recovery.status === 'clean'
    && recovery.backend === backend
    && recovery.cleanup?.succeeded === true
    && recovery.cleanup?.retained === false
    && recovery.resources?.backendState === 'released'
    && recovery.resources?.buildContainer?.running !== true
    && Array.isArray(recovery.resources?.locks)
    && recovery.resources.locks.every(resource => resource.released === true);
}

export function runCampaignAdmission(plan, directory,
  { env = process.env, preflight = runPreflight, now = new Date().toISOString(),
    uuid = randomUUID } = {}) {
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const reports = [];
  for (const adapter of [...new Set(plan.agents.map(agent => agent.adapter))].sort()) {
    reports.push(preflight({
      backends: plan.stacks.map(stack => stack.id),
      track: plan.definition.track,
      levels: `${Math.min(...plan.definition.levels)}-${Math.max(...plan.definition.levels)}`,
      levelList: plan.definition.levels,
      runIndex: 0,
      agentAdapter: adapter,
      packIds: plan.definition.selection.packs,
      checkKeys: plan.definition.selection.checks,
      smoke: true,
      image: plan.definition.runtime.buildImage ?? executionEnv.STACK_BENCH_IMAGE,
      resultsDir: resolve(directory),
    }, { env: executionEnv }));
  }
  const id = `${plan.id}-admission-${now.replace(/[-:.TZ]/g, '').slice(0, 14)}-${uuid()}`;
  const payload = validateCampaignAdmission({ schemaVersion: 1, campaignId: plan.id,
    campaignSha256: plan.contentSha256, createdAt: now,
    ok: reports.every(report => report.ok),
    runtime: plan.definition.runtime,
    agents: plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model,
      identity: agent.identity })),
    conditions: plan.conditions,
    reports }, plan, directory);
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  writeArtifact(path, { kind: 'campaign_admission', id,
    identities: emptyArtifactIdentities({ experiment: {
      id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
    } }), payload });
  return { id, path, payload };
}

export function prepareCampaign(campaignFile, directory) {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try { return initializeCampaignDirectory(plan, directory); }
  finally { releaseCampaignLock(lock); }
}

export function reconcileCampaign(campaignFile, directory,
  { rescue = rescueSupervisedLease } = {}) {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    const { state } = inspectCampaign(initialized.paths.root);
    const attempt = state.attempts.find(item => item.status === 'running');
    if (!attempt) throw new Error('campaign has no running attempt to reconcile');
    const execution = attempt.executions.at(-1);
    const output = contained(initialized.paths.root, execution.output, 'attempt output');
    const supervisorState = contained(initialized.paths.root,
      join('.private', `${execution.id}.supervisor.json`), 'supervisor state');
    if (existsSync(supervisorState)) {
      rescue(supervisorState, output);
    } else if (!publicRecoveryProvesCleanup(output, attempt.plan.stack)) {
      throw new Error('running attempt has neither private supervisor authority nor public clean recovery proof');
    }
    const reconciled = markInterruptedExecution(state, execution.id, {
      reason: 'controller ended before recording completion; exact-owned cleanup was proven',
    });
    writeCampaignState(initialized.paths.state, plan, reconciled);
    return reconciled;
  } finally { releaseCampaignLock(lock); }
}

export async function executeCampaign(campaignFile, directory,
  { allowDraft = false, env = process.env, execute = runBounded,
    admit = runCampaignAdmission } = {}) {
  const plan = compileCampaignFile(resolve(campaignFile));
  if (plan.state !== 'frozen' && !allowDraft) {
    throw new Error('campaign execution requires a frozen plan; draft plans are inspection-only');
  }
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    let { state } = inspectCampaign(initialized.paths.root);
    if (state.attempts.some(attempt => attempt.status === 'running')) {
      throw new Error('campaign has an unresolved running attempt; prove its owned resources are clean before reconciliation');
    }
    if (!state.attempts.some(attempt => attempt.status === 'pending')) return state;
    const admission = admit(plan, initialized.paths.root, { env: executionEnv });
    if (!admission?.payload?.ok || typeof admission.id !== 'string' || !admission.id) {
      throw new Error('campaign-wide preflight admission failed; no attempt was claimed');
    }
    const invalidAtStart = state.summary.invalid;
    while (true) {
      const next = claimNextAttempt(state, { admissionId: admission.id });
      if (!next.claim) return next.state;
      state = next.state;
      writeCampaignState(initialized.paths.state, plan, state);
      const output = contained(initialized.paths.root, next.claim.output, 'attempt output');
      // Preflight bind-mounts the exact attempt output to prove that evidence is
      // durable. The first execution's parent may already exist, while a retry's
      // execution-N directory never does; create both by the same rule before
      // starting the child so retries cannot fail for a different topology.
      mkdirSync(output, { recursive: true });
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${next.claim.executionId}.supervisor.json`), 'supervisor state');
      const processResult = await execute(process.execPath,
        attemptArgv(plan, next.claim.attempt, output), {
          cwd: ROOT,
          env: { ...executionEnv, STACK_BENCH_SUPERVISOR_STATE: supervisorState },
          stdio: 'inherit',
          logs: { stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log') },
          timeoutMs: plan.definition.budgets.attemptTimeoutMinutes * 60_000,
        });
      processResult.buildImage = executionEnv.STACK_BENCH_IMAGE;
      writeArtifact(join(output, 'process.json'), { kind: 'campaign_process',
        id: `${next.claim.executionId}-process`,
        attempt: { id: next.claim.executionId, parentId: next.claim.attempt.id },
        identities: emptyArtifactIdentities({ experiment: {
          id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
        } }),
        payload: { schemaVersion: 1, executionId: next.claim.executionId,
          exitCode: processResult.code ?? null, signal: processResult.signal ?? null,
          timedOut: processResult.timedOut === true,
          streams: processResult.logs ? Object.fromEntries(Object.entries(processResult.logs)
            .map(([name, log]) => [name, { ...log, path: `process.${name}.log` }])) : null } });
      let cleanupError = null;
      if (!processResult.ok && existsSync(supervisorState)) {
        try { rescueSupervisedLease(supervisorState, output); }
        catch (error) { cleanupError = error; }
      }
      const result = cleanupError
        ? { exitCode: processResult.code, timedOut: false, run: { outcome: {
          kind: 'harness_failure', reason: `attempt cleanup failed: ${cleanupError.message}` } } }
        : readAttemptResult(plan, next.claim.attempt, output, processResult);
      state = finishCampaignExecution(state, next.claim.executionId,
        result, {
          retries: plan.definition.attemptPolicy.retries,
          retryOn: plan.definition.attemptPolicy.retryOn,
        });
      writeCampaignState(initialized.paths.state, plan, state);
      if (state.summary.invalid > invalidAtStart) return state;
    }
  } finally {
    releaseCampaignLock(lock);
  }
}
