import type { StackOperationHandler } from './stack-adapter-contract.mjs';

export function hostedStopScript(port: number | string): string;
export const captureHostedDiagnostics: StackOperationHandler;
export const activateHosted: StackOperationHandler;
export const activateSpacetime: StackOperationHandler;
export const controlSpacetime: StackOperationHandler;
export function controlHosted(input: unknown): Promise<unknown>;
