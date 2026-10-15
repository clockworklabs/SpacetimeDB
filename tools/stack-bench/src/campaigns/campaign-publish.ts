import { existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';
import { readCampaignState } from './campaign-scheduler.js';
import { readCampaignLock } from './campaign-lock.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { campaignChildPath, CAMPAIGN_FILE } from './campaign-path.js';
import { readCampaignReport, validateCampaignReport } from './campaign-report.js';
import { campaignProgressionOwner } from './campaign-compiler.js';
import { ARTIFACT_FILE, readArtifact } from '../evidence/artifacts.js';
import { dependencyCompletionBreakdown } from '../progression/dependency-score.js';
import {
  compileProgressionInput,
  dependencyRuntimeDefinition,
} from '../progression/progression-definition.js';
import { readProgressionState } from '../progression/progression-state.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';

const UPLOAD_VERSION = 1;
const MAX_BYTES = 10 * 1024 * 1024;

export function createCampaignUpload(directory: string, suite: string) {
  if (!/^[a-z0-9][a-z0-9.-]{0,99}$/.test(suite))
    throw new Error('publish requires a valid suite slug');
  if (readCampaignLock(directory))
    throw new Error('cannot publish while campaign control is active');
  const current = readCampaignState(directory, { requireCurrentInputs: false });
  const { plan, state } = current;
  if (
    plan.state !== 'frozen' ||
    plan.definition.mode.id !== 'dependency' ||
    !plan.featureCatalog ||
    !plan.dependencyPolicy
  ) {
    throw new Error('publish requires a frozen dependency campaign');
  }
  if (
    !['completed', 'attention-required'].includes(state.status) ||
    state.attempts.some(
      attempt =>
        !['completed', 'invalid'].includes(attempt.status) ||
        !attempt.executions.length ||
        attempt.executions.some(execution => execution.status === 'running')
    )
  ) {
    throw new Error(
      'publish requires a finished campaign with every planned attempt accounted for'
    );
  }
  const reportPath = campaignChildPath(
    directory,
    join('report', CAMPAIGN_FILE.reportJson),
    'campaign report'
  );
  if (!existsSync(reportPath)) throw new Error('run campaign report before publishing');
  const storedReport = validateCampaignReport(
    readArtifact(reportPath, { expectedKind: 'campaign_report' }).payload
  );
  const { report } = readCampaignReport(directory);
  if (storedReport.contentSha256 !== report.contentSha256) {
    throw new Error('retained evidence changed; run campaign report again before publishing');
  }
  const progression = compileProgressionInput(
    dependencyRuntimeDefinition(plan.featureCatalog, plan.dependencyPolicy)
  );
  const attempts = report.attempts.map(attempt => {
    const execution = attempt.executions.at(-1)!;
    if (
      !['completed', 'invalid'].includes(attempt.status) ||
      typeof execution.completedAt !== 'string' ||
      typeof execution.startedAt !== 'string' ||
      typeof execution.outcome !== 'string'
    )
      throw new Error(`attempt ${attempt.id} has incomplete evidence`);
    const invalid = attempt.status === 'invalid';
    let featuresPassed: number | null = null;
    const featuresTotal = progression.definition.nodes.length;
    if (!invalid) {
      // The report orders attempts for reading; the run record is found by attempt id.
      const recorded = state.attempts.find(item => item.plan.id === attempt.id);
      if (!recorded) throw new Error(`attempt ${attempt.id} has no campaign state`);
      const output = campaignChildPath(
        directory,
        recorded.executions.at(-1)!.output,
        'campaign execution'
      );
      const stored = readProgressionState(join(output, ARTIFACT_FILE.progressionState), {
        progression,
        featureCatalogIdentity: plan.featureCatalog!.identity,
        dependencyPolicyIdentity: plan.dependencyPolicy!.identity,
        owner: campaignProgressionOwner(plan, attempt, { workspace: true }),
      });
      if (stored.state.phase !== 'terminal')
        throw new Error(`attempt ${attempt.id} has no final feature measurement`);
      featuresPassed = dependencyCompletionBreakdown(stored.state).featureCompletion.passed;
    }
    const excluded =
      invalid || plan.definition.attemptPolicy.excludeFromAnalysis.includes(execution.outcome);
    const cost = invalid
      ? attempt.spend
      : {
          status:
            attempt.metrics?.totalCostUsd != null
              ? 'exact'
              : attempt.metrics?.totalCostUpperBoundUsd != null
                ? 'upper-bound'
                : 'unknown',
          costUsd: attempt.metrics?.totalCostUsd ?? attempt.metrics?.totalCostUpperBoundUsd ?? null,
        };
    return {
      attemptId: attempt.id,
      stack: attempt.stack,
      agent: attempt.agentAdapter,
      model: attempt.model,
      effort: attempt.effort ?? 'default',
      repetition: attempt.repetition,
      status: invalid ? ('invalid' as const) : ('completed' as const),
      outcome: execution.outcome,
      featuresPassed,
      featuresTotal,
      costUsd: cost.costUsd,
      costStatus: cost.status,
      startedAt: execution.startedAt,
      completedAt: execution.completedAt,
      excluded,
      excludedReason: excluded
        ? redactCredentials(
            `${execution.outcome}: ${typeof execution.reason === 'string' ? execution.reason : 'excluded by campaign policy'}`
          ).slice(0, 1000)
        : null,
    };
  });
  if (!attempts.length || attempts.length > 10000)
    throw new Error('publish supports 1 to 10,000 attempts per upload');
  const payload = {
    version: UPLOAD_VERSION,
    suite,
    campaignId: plan.id,
    campaignSha256: plan.contentSha256,
    reportSha256: report.contentSha256,
    catalogSha256: plan.featureCatalog.identity.contentSha256,
    gradingStatus: report.scope.grading.status,
    mode: 'dependency',
    frozen: true,
    finished: true,
    attempts,
  };
  if (Buffer.byteLength(JSON.stringify(payload)) > MAX_BYTES)
    throw new Error('upload exceeds 10 MiB');
  const after = readCampaignState(directory, { requireCurrentInputs: false });
  if (
    readCampaignLock(directory) ||
    current.plan.contentSha256 !== after.plan.contentSha256 ||
    canonicalDefinitionJson(current.state) !== canonicalDefinitionJson(after.state)
  ) {
    throw new Error('campaign state changed while preparing the upload; retry after reporting');
  }
  return payload;
}

export async function publishCampaign(
  directory: string,
  options: { suite: string; output?: string; to?: string }
) {
  if (!!options.output === !!options.to)
    throw new Error('publish requires exactly one of --out or --to');
  const payload = createCampaignUpload(directory, options.suite);
  if (options.output) {
    const content = `${JSON.stringify(payload, null, 2)}\n`;
    if (Buffer.byteLength(content) > MAX_BYTES) throw new Error('upload file exceeds 10 MiB');
    writeFileSync(options.output, content, { flag: 'wx' });
    return { output: options.output, runs: payload.attempts.length };
  }
  const site = new URL(options.to!);
  if (site.username || site.password || site.search || site.hash || site.pathname !== '/') {
    throw new Error('--to must be a site origin without credentials, query or path');
  }
  if (
    site.protocol !== 'https:' &&
    !(site.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(site.hostname))
  ) {
    throw new Error('uploads require HTTPS, except on localhost');
  }
  const key = process.env.STACKBENCH_UPLOAD_KEY;
  if (!key) throw new Error('set STACKBENCH_UPLOAD_KEY before publishing');
  const response = await fetch(new URL('/api/uploads', site), {
    method: 'POST',
    redirect: 'error',
    signal: AbortSignal.timeout(30000),
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${key}` },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    await response.body?.cancel();
    throw new Error(`upload rejected with HTTP ${response.status}; retained evidence is unchanged`);
  }
  const result = z
    .object({
      version: z.literal(UPLOAD_VERSION),
      uploadId: z.string().min(1).max(200),
      status: z.enum(['pending', 'published', 'withdrawn']),
      duplicate: z.boolean(),
      runs: z.number().int().positive(),
    })
    .safeParse(await response.json());
  if (!result.success || result.data.runs !== payload.attempts.length) {
    throw new Error('site returned an invalid upload acknowledgement');
  }
  return result.data;
}
