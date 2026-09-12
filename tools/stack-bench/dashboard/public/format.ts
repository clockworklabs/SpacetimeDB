// One spelling per value. Every figure the dashboard prints goes through here,
// so a percentage, a duration and a dash look the same on every page.

import type { SheetAttempt } from '../dashboard-views.js';
import type { CostEvidence } from '../../src/evidence/cost-proof.js';
import { statusWord } from '../../src/evidence/status-words.js';
import { outputSilentMinutes } from './metrics.js';

export { statusWord };

const SILENCE_MINUTES = 10;

export const STACK_LABEL: Record<string, string> = { spacetime: 'SpacetimeDB',
  postgres: 'PostgreSQL', mongodb: 'MongoDB' };
export const DASH = '—';

export function esc(value: unknown): string {
  return String(value ?? '').replace(/[&<>"']/g, character =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character] ?? character);
}

export function stackLabel(stack: string): string {
  return STACK_LABEL[stack] ?? stack;
}

export function metricLabel(label: string, description: string | undefined): string {
  if (!description) return `<span class="label">${esc(label)}</span>`;
  const id = `help-${label.toLowerCase().replaceAll(' ', '-')}`;
  return `<button class="metric-help label" type="button" popovertargetaction="show" popovertarget="${esc(id)}" aria-label="About ${esc(label)}" aria-describedby="${esc(id)}">${esc(label)}</button>`
    + `<div class="metric-tooltip" id="${esc(id)}" popover role="tooltip">${esc(description)}</div>`;
}

export function pct(value: number | null | undefined): string {
  return value == null ? DASH : `${Math.round(value)}%`;
}

export function num(value: number | null | undefined): string {
  return value == null ? DASH : String(Math.round(value));
}

// One value: the count and the total it is out of.
export function ratio(used: number | null | undefined, budget: number | null | undefined): string {
  if (used == null) return DASH;
  return budget == null ? String(used) : `${used} / ${budget}`;
}

export function money(value: number | null | undefined): string {
  if (value == null) return DASH;
  return `$${value.toFixed(2)}`;
}

export function spend(value: CostEvidence & { knownCostUsd?: number }, pending = false, liveSpend?: number): string {
  return (liveSpend !== undefined
    ? `<span title="Live estimate from reported response usage; final receipts replace this value">~${money(liveSpend)}</span>`
    : value.status === 'unknown' ? value.knownCostUsd
      ? `<span title="Recorded spend; final accounting is incomplete">${money(value.knownCostUsd)} recorded</span>` : 'Unknown'
    : `${value.status === 'upper-bound' ? '≤' : ''}${money(value.costUsd)}`)
    + (pending ? ' <span class="spend-pending dot a" role="img" aria-label="Cost still updating" title="Cost still updating"></span>' : '');
}

export function duration(seconds: number | null | undefined): string {
  if (seconds == null) return DASH;
  const minutes = Math.round(seconds / 60);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

// Wall time for the current execution, separate from the measured run duration.
export function elapsed(startedAt: string | null, completedAt: string | null,
  now = Date.now()): string {
  if (startedAt === null) return DASH;
  const start = Date.parse(startedAt);
  const end = completedAt === null ? now : Date.parse(completedAt);
  if (!Number.isFinite(start) || !Number.isFinite(end)) return DASH;
  const seconds = Math.floor(Math.max(0, end - start) / 1000);
  const minutes = Math.floor(seconds / 60);
  return `${minutes >= 60 ? `${Math.floor(minutes / 60)}h ` : ''}${minutes % 60}m ${seconds % 60}s`;
}

export function executionClock(startedAt: string | null, completedAt: string | null): string {
  return `<span title="Elapsed wall time, including pauses"${startedAt && !completedAt ? ` data-started-at="${esc(startedAt)}"` : ''}>${elapsed(startedAt, completedAt)}</span>`;
}

export function since(value: string | null | undefined, now = Date.now()): string {
  if (!value) return DASH;
  const minutes = Math.max(0, Math.floor((now - Date.parse(value)) / 60000));
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 60 * 48) return `${Math.floor(minutes / 60)}h`;
  return `${Math.floor(minutes / 1440)}d`;
}

export function phrase(attempt: SheetAttempt, now = Date.now()): string {
  const parts = [attempt.phase];
  const silent = outputSilentMinutes(attempt, now);
  if (silent >= SILENCE_MINUTES) parts.push(`no agent activity observed for ${silent}m`);
  return parts.join(' · ');
}

// depth 3 · 1×  /  L1–L3 · 3×
export function shape(mode: string, levels: readonly number[], repetitions: number): string {
  const depth = levels.length ? Math.max(...levels) : 0;
  const span = mode === 'dependency' ? `depth ${depth}`
    : levels.length > 1 ? `L${Math.min(...levels)}–L${depth}` : `L${depth}`;
  return `${span} · ${repetitions}×`;
}

export function modelLabel(model?: string): string {
  return ({ 'claude-fable-5-1': 'Fable 5.1', 'claude-opus-5': 'Opus 5', 'gpt-5.6-sol': 'Sol', 'gpt-6-astra': 'Astra' } as Record<string, string>)[model ?? ''] ?? model ?? '';
}

export function completionLabel(attempt: Pick<SheetAttempt, 'status' | 'excluded' | 'completion'>): string {
  if (attempt.excluded && attempt.status !== 'running' && attempt.status !== 'pending') {
    return attempt.status === 'completed' ? 'Excluded' : 'Incomplete';
  }
  return attempt.completion ? ratio(attempt.completion.passed, attempt.completion.selected) : DASH;
}

export function runLabel(attempt: Pick<SheetAttempt, "repetition" | "model" | "effort">, showRepetition = true): string {
  return `${modelLabel(attempt.model)}${attempt.effort ? ` (${attempt.effort})` : ''}${showRepetition ? ` · Rep ${attempt.repetition}` : ''}`;
}
