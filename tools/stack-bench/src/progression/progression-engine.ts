import { dependencyModePolicy } from './dependency-mode.js';
import type {
  ProgressionTerminalOutcome,
} from './progression-state.js';

export interface ProgressionTerminalAction extends Record<string, unknown> {
  type: 'terminal';
  outcome: ProgressionTerminalOutcome;
}

export interface ProgressionWorkAction extends Record<string, unknown> {
  type: 'build' | 'repair';
  level: number;
  repair: {
    nodeIds: string[];
    remaining: number;
    grantId?: string;
    // The last completed repair was charged but never graded. Grade its
    // preserved source before starting another coding session.
    awaitingGrade?: true;
  };
  prompt: unknown;
  grading: unknown;
}

export type ProgressionAction = ProgressionTerminalAction | ProgressionWorkAction;

export interface ProgressionPolicy<
  TDefinition = unknown,
  TState = unknown,
  TAction = unknown,
  TScore = unknown,
  TGradingSelection = unknown,
> {
  id: string;
  compile(definition: unknown): TDefinition;
  initialize(definition: unknown): TState;
  activeNodes(state: TState): unknown;
  promptSelection(state: TState): unknown;
  gradingSelection(state: TState): TGradingSelection;
  recordResult(state: TState, result: unknown): TState;
  grantRepairs(state: TState, grant: unknown): TState;
  replay(definition: unknown, events: unknown[]): TState;
  nextAction(state: TState): TAction;
  score(state: TState): TScore;
}

export const progressionEngine = dependencyModePolicy;
