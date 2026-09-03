export const REPAIR_SELECTIONS = ['feature', 'batch'] as const;

export type RepairSelection = typeof REPAIR_SELECTIONS[number];

export interface RepairBudget extends Record<string, unknown> {
  total?: number;
  perFeature?: number;
  perDepth?: { count: number; carry: boolean };
}

export interface RepairPlan extends Record<string, unknown> {
  selection: RepairSelection;
  budget: RepairBudget;
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
    if (!['selection', 'budget'].includes(key)) throw new Error(`${at}.${key} is unknown`);
  }
  if (!REPAIR_SELECTIONS.includes(input.selection as RepairSelection)) {
    throw new Error(`${at}.selection must be "feature" or "batch"`);
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
  return { selection: input.selection as RepairSelection, budget };
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
