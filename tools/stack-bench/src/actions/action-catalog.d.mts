import type { ActionPlugin, ActionRegistry } from './action-contract.js';

export const ACTION_REGISTRY: ActionRegistry;

export function legacyActionPlugin(id: string): ActionPlugin;
