import type { StackOperationHandler } from './stack-adapter-contract.mjs';

export const postgresConnectionUrl: StackOperationHandler;
export const mongoDbConnectionUrl: StackOperationHandler;
export const noConnectionUrl: StackOperationHandler;
export const spacetimeSetupMetadata: StackOperationHandler;
export const postgresSetupMetadata: StackOperationHandler;
export const mongoDbSetupMetadata: StackOperationHandler;
export const emptySetupMetadata: StackOperationHandler;
export const spacetimeBuildContainerPlan: StackOperationHandler;
export function standardBuildContainerPlan(input?: unknown): unknown;
