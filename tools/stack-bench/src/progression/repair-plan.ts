export const REPAIR_SELECTIONS = ['feature', 'batch'] as const;

export type RepairSelection = typeof REPAIR_SELECTIONS[number];

// Why a feature can no longer be repaired. The first three name the configured
// limit that ran out, most specific first; the last is the stall detector.
export const REPAIR_EXHAUSTION_REASONS = [
  'feature-repairs-exhausted',
  'depth-repairs-exhausted',
  'total-repairs-exhausted',
  'repeated-findings',
] as const;

export type RepairExhaustionReason = typeof REPAIR_EXHAUSTION_REASONS[number];

export type RepairLimitExhaustion = Exclude<RepairExhaustionReason, 'repeated-findings'>;

export interface RepairBudget extends Record<string, unknown> {
  total?: number;
  perFeature?: number;
  perDepth?: { count: number; carry: boolean };
}

// The order failed features are repaired in within a dependency depth:
// the catalog's declared order, or a permutation drawn once per campaign
// from its ordering seed and shared by every stack in that campaign.
export const REPAIR_ORDERS = ['declared', 'shuffled'] as const;

export type RepairOrder = typeof REPAIR_ORDERS[number];

export interface RepairPlanInput {
  selection: RepairSelection;
  budget: RepairBudget;
  order?: RepairOrder;
}

export interface RepairPlan extends Record<string, unknown> {
  selection: RepairSelection;
  budget: RepairBudget;
  order: RepairOrder;
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function integer(value: unknown, at: string): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    throw new Error(`${at} must be a non-negative safe integer`);
  }
  return Number(value);
}

export function validateRepairPlan(input: unknown, at = 'repair'): RepairPlan {
  if (!object(input)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(input)) {
    if (!['selection', 'budget', 'order'].includes(key)) throw new Error(`${at}.${key} is unknown`);
  }
  if (!REPAIR_SELECTIONS.includes(input.selection as RepairSelection)) {
    throw new Error(`${at}.selection must be "feature" or "batch"`);
  }
  const order = input.order === undefined ? 'declared' : input.order;
  if (!REPAIR_ORDERS.includes(order as RepairOrder)) {
    throw new Error(`${at}.order must be "declared" or "shuffled"`);
  }
  if (!object(input.budget)) throw new Error(`${at}.budget must be an object`);
  const budgetKeys = Object.keys(input.budget);
  if (budgetKeys.length === 0) throw new Error(`${at}.budget must contain a limit`);
  for (const key of budgetKeys) {
    if (!['total', 'perFeature', 'perDepth'].includes(key)) {
      throw new Error(`${at}.budget.${key} is unknown`);
    }
  }
  const budget: RepairBudget = {};
  if (input.budget.total !== undefined) {
    budget.total = integer(input.budget.total, `${at}.budget.total`);
  }
  if (input.budget.perFeature !== undefined) {
    budget.perFeature = integer(input.budget.perFeature, `${at}.budget.perFeature`);
  }
  if (input.budget.perDepth !== undefined) {
    if (!object(input.budget.perDepth)) {
      throw new Error(`${at}.budget.perDepth must be an object`);
    }
    for (const key of Object.keys(input.budget.perDepth)) {
      if (!['count', 'carry'].includes(key)) {
        throw new Error(`${at}.budget.perDepth.${key} is unknown`);
      }
    }
    if (typeof input.budget.perDepth.carry !== 'boolean') {
      throw new Error(`${at}.budget.perDepth.carry must be true or false`);
    }
    budget.perDepth = {
      count: integer(input.budget.perDepth.count, `${at}.budget.perDepth.count`),
      carry: input.budget.perDepth.carry,
    };
  }
  if (Object.keys(budget).length === 0) throw new Error(`${at}.budget must contain a limit`);
  return { selection: input.selection as RepairSelection, budget, order: order as RepairOrder };
}

export function repairBudgetLimit(plan: RepairPlan,
  { features = 1, depths = 1 }: { features?: number; depths?: number } = {}): number {
  const limits = [
    plan.budget.total,
    plan.budget.perFeature === undefined ? undefined : plan.budget.perFeature * features,
    plan.budget.perDepth === undefined ? undefined : plan.budget.perDepth.count * depths,
  ].filter((limit): limit is number => limit !== undefined);
  return Math.min(...limits);
}
