// One spelling per value. Every figure the dashboard prints goes through here,
// so a percentage, a duration and a dash look the same on every page.

import type { SheetAttempt } from '../dashboard-views.js';
import { stallRounds } from './metrics.js';

const SILENCE_MINUTES = 10;

export const STACK_LABEL: Record<string, string> = { spacetime: 'SpacetimeDB',
  postgres: 'PostgreSQL', mongodb: 'MongoDB' };
const STATUS_WORD: Record<string, string> = { prepared: 'ready', 'attention-required':
  'needs attention', pending: 'queued', invalid: 'excluded', interrupted: 'interrupted' };
export const DASH = '—';

export function esc(value: unknown): string {
  return String(value ?? '').replace(/[&<>"']/g, character =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character] ?? character);
}

export function stackLabel(stack: string): string {
  return STACK_LABEL[stack] ?? stack;
}

export function statusWord(status: string): string {
  return STATUS_WORD[status] ?? status;
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
  return budget == null ? String(used) : `${used}<i>/ ${budget}</i>`;
}

export function money(value: number | null | undefined): string {
  if (value == null) return DASH;
  return value >= 10 ? `$${Math.round(value)}` : `$${value.toFixed(2)}`;
}

export function duration(seconds: number | null | undefined): string {
  if (seconds == null) return DASH;
  const minutes = Math.round(seconds / 60);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function since(value: string | null | undefined, now = Date.now()): string {
  if (!value) return DASH;
  const minutes = Math.max(0, Math.floor((now - Date.parse(value)) / 60000));
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 60 * 48) return `${Math.floor(minutes / 60)}h`;
  return `${Math.floor(minutes / 1440)}d`;
}

// The phase, with the stall the operator would otherwise find by diffing round
// logs: three identical grades, or ten minutes without output.
export function phrase(attempt: SheetAttempt, now = Date.now()): string {
  const parts = [attempt.phase];
  const flat = stallRounds(attempt.climb);
  if (flat) parts.push(`same score for ${flat} grades`);
  const silent = attempt.status === 'running' && attempt.logUpdatedAt
    ? Math.floor((now - Date.parse(attempt.logUpdatedAt)) / 60000) : 0;
  if (silent >= SILENCE_MINUTES) parts.push(`no output for ${silent}m`);
  return parts.join(' · ');
}

// depth 3 · 1×  /  L1–L3 · 3×
export function shape(mode: string, levels: readonly number[], repetitions: number): string {
  const depth = levels.length ? Math.max(...levels) : 0;
  const span = mode === 'dependency' ? `depth ${depth}`
    : levels.length > 1 ? `L${Math.min(...levels)}–L${depth}` : `L${depth}`;
  return `${span} · ${repetitions}×`;
}
