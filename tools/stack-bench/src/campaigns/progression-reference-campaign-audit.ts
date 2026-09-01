import { relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { recipeReleaseIdentity, resolveRecipeRelease }
  from '../composition/recipe-release.js';
import type { RecipeBinding, RecipeRelease } from '../composition/recipe-release.js';
import { loadTrack } from '../composition/tracks.js';
import type { Track } from '../composition/tracks.js';
import { auditProgressionReferenceRun }
  from '../progression/progression-reference-audit.js';
import type { ProgressionReferenceAuditReport }
  from '../progression/progression-reference-audit.js';
import type { ProgressionOwner } from '../progression/progression-state.js';
import { readCampaignState } from './campaign-scheduler.js';
import { campaignProgressionOwner } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.js';
import type { ProgressionInput } from '../progression/progression-definition.js';

interface CampaignExecutionView {
  id: string;
  output: string;
}

interface CampaignAttemptView {
  plan: CampaignAttemptPlan;
  status: string;
  executions: CampaignExecutionView[];
}

interface CampaignAuditStore {
  paths: { root: string };
  plan: CompiledCampaignPlan;
  state: { status: string; attempts: CampaignAttemptView[] };
}

interface CampaignRunAuditInput {
  outputDir: string;
  progression: ProgressionInput;
  featureCatalogIdentity: unknown;
  dependencyPolicyIdentity: unknown;
  owner: ProgressionOwner;
  recipeBindings: Map<number, RecipeBinding>;
  release: RecipeRelease;
}

interface RunAuditSummary {
  ok: boolean;
  graphOwned: ProgressionReferenceAuditReport['graphOwned'];
  finalCatalogAudit: {
    status: string;
    checks: number;
    points: number;
    zeroPointChecks: number;
    additionalChecks: Array<{ stableKey: string; points: number }>;
  };
}
type AuditRun = (input: CampaignRunAuditInput) => RunAuditSummary;
type ReadState = (directory: string,
  options?: { requireCurrentInputs?: boolean }) => CampaignAuditStore;
type ResolveRelease = (track: Track, level: number, requested: string) => RecipeBinding | null;

interface CampaignAuditOptions {
  auditRun?: AuditRun;
  readState?: ReadState;
  resolveRelease?: ResolveRelease;
}

interface ReferenceCampaignAttemptAudit {
  id: string;
  stack: string;
  execution: string;
  ok: boolean;
  progressionGraph: {
    complete: boolean;
    nodes: { covered: number; total: number };
    checks: { covered: number; total: number };
    points: number;
    missingNodes: string[];
    missingChecks: string[];
  };
  fullRecipeCatalog: {
    status: string;
    checks: number;
    points: number;
    zeroPointChecks: number;
    outsideGraph: Array<{ stableKey: string; points: number }>;
  };
}

export interface ReferenceCampaignAudit {
  ok: boolean;
  [key: string]: unknown;
}

export interface DetailedReferenceCampaignAudit extends ReferenceCampaignAudit {
  schemaVersion: 1;
  campaign: { id: string; version: string; sha256: string };
  attempts: ReferenceCampaignAttemptAudit[];
}

function childPath(root: string, path: string): string {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const relation = relative(absoluteRoot, absolute);
  if (!relation || relation === '..' || relation.startsWith(`..${sep}`)) {
    throw new Error('reference audit execution output is outside the campaign directory');
  }
  return absolute;
}

function exactRecipeBindings(plan: CompiledCampaignPlan,
  resolveRelease: ResolveRelease): { bindings: Map<number, RecipeBinding>; release: RecipeRelease } {
  const track = loadTrack(plan.definition.track);
  const bindings = new Map<number, RecipeBinding>();
  let release: RecipeRelease | null = null;
  for (const planned of plan.bindings) {
    const reference = `${planned.recipe.id}@${planned.recipe.version}`;
    const binding = resolveRelease(track, planned.level, reference);
    if (!binding || canonicalDefinitionJson(recipeReleaseIdentity(binding.release))
      !== canonicalDefinitionJson(planned.recipe)) {
      throw new Error(`reference audit recipe binding for L${planned.level} changed after planning`);
    }
    if (release !== null && (binding.release.id !== release.id
      || binding.release.version !== release.version
      || binding.release.contentSha256 !== release.contentSha256)) {
      throw new Error('reference audit requires one exact recipe release across all levels');
    }
    release ??= binding.release;
    bindings.set(planned.level, binding);
  }
  if (release === null) throw new Error('reference audit has no recipe bindings');
  return { bindings, release };
}

function summarizeAttempt(attempt: CampaignAttemptPlan, execution: CampaignExecutionView,
  audit: RunAuditSummary): ReferenceCampaignAttemptAudit {
  return {
    id: attempt.id,
    stack: attempt.stack,
    execution: execution.id,
    ok: audit.ok,
    progressionGraph: {
      complete: audit.graphOwned.complete,
      nodes: { covered: audit.graphOwned.coveredNodes, total: audit.graphOwned.nodes },
      checks: { covered: audit.graphOwned.coveredChecks, total: audit.graphOwned.checks },
      points: audit.graphOwned.points,
      missingNodes: audit.graphOwned.missingNodes,
      missingChecks: audit.graphOwned.missingChecks,
    },
    fullRecipeCatalog: {
      status: audit.finalCatalogAudit.status,
      checks: audit.finalCatalogAudit.checks,
      points: audit.finalCatalogAudit.points,
      zeroPointChecks: audit.finalCatalogAudit.zeroPointChecks,
      outsideGraph: audit.finalCatalogAudit.additionalChecks,
    },
  };
}

export function auditProgressionReferenceCampaign(directory: string, {
  auditRun = auditProgressionReferenceRun,
  readState = readCampaignState,
  resolveRelease = resolveRecipeRelease,
}: CampaignAuditOptions = {}): DetailedReferenceCampaignAudit | null {
  const { plan, state, paths } = readState(directory, { requireCurrentInputs: false });
  if (state.status !== 'completed') {
    throw new Error('reference campaign audit requires a completed campaign');
  }
  const selected = state.attempts.filter(attempt =>
    attempt.plan.mode?.id === 'dependency'
    && attempt.plan.agentAdapter === 'reference-fixture');
  if (selected.length === 0) return null;
  if (!plan.featureCatalog || !plan.dependencyPolicy) {
    throw new Error('reference campaign audit requires a feature catalog and dependency policy');
  }
  const progression = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  const featureCatalogIdentity = plan.featureCatalog.identity;
  const dependencyPolicyIdentity = plan.dependencyPolicy.identity;
  const { bindings, release } = exactRecipeBindings(plan, resolveRelease);
  const attempts = selected.map(attempt => {
    if (attempt.status !== 'completed') {
      throw new Error(`reference campaign attempt ${attempt.plan.id} is not completed`);
    }
    const execution = attempt.executions.at(-1);
    if (!execution) {
      throw new Error(`reference campaign attempt ${attempt.plan.id} has no execution`);
    }
    const audit = auditRun({
      outputDir: childPath(paths.root, execution.output),
      progression,
      featureCatalogIdentity,
      dependencyPolicyIdentity,
      owner: campaignProgressionOwner(plan, attempt.plan, { workspace: true }),
      recipeBindings: bindings,
      release,
    });
    return summarizeAttempt(attempt.plan, execution, audit);
  });
  return {
    schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    ok: attempts.every(attempt => attempt.ok),
    attempts,
  };
}

export function formatProgressionReferenceCampaignAudit(
  report: DetailedReferenceCampaignAudit): string;
export function formatProgressionReferenceCampaignAudit(
  report: ReferenceCampaignAudit | null): string | null;
export function formatProgressionReferenceCampaignAudit(
  report: ReferenceCampaignAudit | null): string | null {
  if (report === null) return null;
  if (!Array.isArray(report.attempts)) {
    throw new Error('reference campaign audit report has no attempts');
  }
  const attempts = report.attempts as ReferenceCampaignAttemptAudit[];
  const incomplete = !report.ok && attempts.every(attempt =>
    attempt.progressionGraph.complete && attempt.fullRecipeCatalog.status === 'not-run');
  const lines = [`Reference progression audit: ${report.ok ? 'PASS' : incomplete ? 'INCOMPLETE' : 'FAIL'}`];
  for (const attempt of attempts) {
    const graph = attempt.progressionGraph;
    const catalog = attempt.fullRecipeCatalog;
    lines.push(`${attempt.stack}: graph ${graph.nodes.covered}/${graph.nodes.total} nodes, `
      + `${graph.checks.covered}/${graph.checks.total} checks, ${graph.points} points; `
      + `full recipe catalog ${catalog.status}, ${catalog.checks} checks, ${catalog.points} points, `
      + `${catalog.outsideGraph.length} outside the graph`);
  }
  return lines.join('\n');
}
