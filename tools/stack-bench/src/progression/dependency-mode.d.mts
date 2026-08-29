import type {
  CompiledProgressionDefinition,
  CompiledProgressionNode,
} from './progression-definition.js';

export const DEPENDENCY_MODE_SCHEMA_VERSION: number;
export const DEPENDENCY_MODE_POLICY: string;
export const FEATURE_CATALOG_SCHEMA_VERSION: number;
export const DEFAULT_UNCHANGED_FAILURE_LIMIT: number;

export function compileFeatureCatalog(
  input: unknown,
  options?: { source?: string },
): CompiledProgressionDefinition;
export function compileDependencyMode(
  input: unknown,
  options?: { source?: string },
): CompiledProgressionDefinition;

export interface DependencyState extends Record<string, unknown> {
  definition: CompiledProgressionDefinition;
  nodes: Record<string, { status: string; [key: string]: unknown }>;
  events: unknown[];
  attempts: Array<Record<string, unknown>>;
}

export function initializeDependencyMode(input: unknown): DependencyState;
export function replayDependencyMode(input: unknown, events?: unknown[]): DependencyState;
export function resumeDependencyMode(snapshot: unknown): DependencyState;
export function recordDependencyResult(state: unknown, result: unknown): DependencyState;
export function grantDependencyStrikes(state: unknown, grant: unknown): DependencyState;
export function nextDependencyAction(state: unknown): Record<string, unknown>;
export function activeDependencyNodes(state: unknown): CompiledProgressionNode[];
export function scoreDependencyMode(state: unknown): unknown;
export const dependencyModePolicy: Record<string, unknown>;
