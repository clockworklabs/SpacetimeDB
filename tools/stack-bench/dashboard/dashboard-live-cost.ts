import { attemptTranscriptFiles } from './dashboard-transcript.js';
import { normalizeClaudeUsage, priceClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import type { PricingRates } from '../src/evidence/pricing-authority.js';

interface UsagePoint { id: string; completedAt: string; costUsd: number; signature: string }
export function liveCostTotal(status: string, observed: number | undefined, saved: number | null): number | undefined {
  return status === 'running' && observed !== undefined && observed >= (saved ?? 0) ? observed : undefined;
}
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

// Display-only usage. Never write these observations into benchmark evidence.
export function responseCosts(text: string, rates: PricingRates, model: string, startedAt: string): UsagePoint[] {
  const points: UsagePoint[] = [];
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    let event: unknown;
    try { event = JSON.parse(line); } catch { throw new Error('Incomplete usage transcript'); }
    if (!object(event) || event.type !== 'assistant' || !object(event.message) || !event.message.usage) continue;
    const message = event.message;
    if (typeof message.stop_reason !== 'string' || !message.stop_reason) continue;
    const timestamp = typeof event.timestamp === 'string' ? Date.parse(event.timestamp) : NaN;
    if (!Number.isFinite(timestamp)) throw new Error('Usage timestamp unavailable');
    if (timestamp < Date.parse(startedAt)) continue;
    if (typeof message.model !== 'string' || !(message.model === model || message.model.startsWith(`${model}-`))) {
      throw new Error('No pinned price for transcript model');
    }
    const id = event.requestId ?? message.id ?? event.uuid;
    if (typeof id !== 'string' || !id) throw new Error('Usage request identity unavailable');
    points.push({ id, completedAt: new Date(timestamp).toISOString(),
      costUsd: priceClaudeUsage(message.usage, rates),
      signature: JSON.stringify([message.model, normalizeClaudeUsage(message.usage)]) });
  }
  return points;
}

export function cumulativeResponseCosts(points: readonly UsagePoint[]): Array<{ completedAt: string; costUsd: number }> {
  const unique = new Map<string, UsagePoint>();
  for (const point of points) {
    const prior = unique.get(point.id);
    if (prior && prior.signature !== point.signature) throw new Error('Conflicting usage for request');
    if (!prior) unique.set(point.id, point);
  }
  let total = 0;
  return [...unique.values()].sort((a, b) => a.completedAt.localeCompare(b.completedAt)).map(point => ({
    completedAt: point.completedAt, costUsd: Number((total += point.costUsd).toFixed(6)),
  }));
}

interface CodexUsageState { session?: string; model?: string; totals?: [number, number, number] }

export function codexResponseCosts(text: string, rates: PricingRates, model: string, startedAt: string): UsagePoint[] {
  const state: CodexUsageState = {};
  const points: UsagePoint[] = [];
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    const event: unknown = JSON.parse(line);
    if (!object(event) || !object(event.payload)) continue;
    const payload = event.payload;
    if (event.type === 'session_meta') {
      if (typeof payload.id !== 'string' || !payload.id) throw new Error('Usage session identity unavailable');
      if (state.session && state.session !== payload.id) throw new Error('Usage session changed');
      state.session = payload.id;
    }
    if (event.type === 'turn_context') state.model = typeof payload.model === 'string' ? payload.model : undefined;
    if (event.type !== 'event_msg' || payload.type !== 'token_count' || payload.info === null) continue;
    const usage = object(payload.info) && object(payload.info.total_token_usage)
      ? payload.info.total_token_usage : {};
    const totals = [usage.input_tokens, usage.cached_input_tokens, usage.output_tokens];
    if (!totals.every(value => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0)
      || Number(totals[1]) > Number(totals[0])) throw new Error('Invalid Codex token usage');
    const counts = totals as [number, number, number];
    const previous = state.totals ?? [0, 0, 0];
    const delta = [counts[0] - previous[0], counts[1] - previous[1], counts[2] - previous[2]] as const;
    if (delta.some(value => value < 0) || delta[1] > delta[0]) throw new Error('Codex usage totals decreased');
    state.totals = counts;
    const timestamp = typeof event.timestamp === 'string' ? Date.parse(event.timestamp) : NaN;
    if (!Number.isFinite(timestamp)) throw new Error('Usage timestamp unavailable');
    if (timestamp < Date.parse(startedAt) || delta.every(value => value === 0)) continue;
    if (!state.session) throw new Error('Usage session identity unavailable');
    if (state.model !== model) throw new Error('No pinned price for transcript model');
    points.push({ id: `${state.session}:${counts.join(':')}`, completedAt: new Date(timestamp).toISOString(),
      costUsd: ((delta[0] - delta[1]) * rates.input + delta[1] * rates.cacheRead + delta[2] * rates.output) / 1e6,
      signature: JSON.stringify([state.model, delta]) });
  }
  return points;
}

const cache = new Map<string, { size: number; modified: number; offset: number; points: UsagePoint[] }>();

export async function liveTranscriptCost(directory: string, adapter: string, rates: PricingRates,
  model: string, startedAt: string) {
  const files = await attemptTranscriptFiles([{ directory, label: 'Execution' }], adapter);
  const activityUpdatedAt = files.length ? new Date(Math.max(...files.map(file => file.modified))).toISOString() : null;
  const points: UsagePoint[] = [];
  for (const file of files) {
    const key = `${directory}/${file.id}/${startedAt}/${JSON.stringify(rates)}/${model}`;
    const prior = cache.get(key);
    if (prior?.size === file.size && prior.modified === file.modified) { points.push(...prior.points); continue; }
    // Read Codex session metadata and cumulative counters together.
    const offsetStart = adapter !== 'codex' && prior && file.size > prior.size ? prior.offset : 0;
    // Bound catch-up work; do not label a partial file as a complete live total.
    if (file.size - offsetStart > 16 * 1024 * 1024) return { activityUpdatedAt, costs: [] };
    const chunks: Buffer[] = [];
    for (let offset = offsetStart; offset < file.size; offset += 256 * 1024) {
      chunks.push(await file.read(offset, Math.min(256 * 1024, file.size - offset)));
    }
    const bytes = Buffer.concat(chunks);
    const end = bytes.lastIndexOf(10);
    const text = end < 0 ? '' : bytes.subarray(0, end + 1).toString();
    const parsed = [...(offsetStart ? prior!.points : []), ...(adapter === 'codex'
      ? codexResponseCosts(text, rates, model, startedAt)
      : responseCosts(text, rates, model, startedAt))];
    if (cache.size >= 128) cache.delete(cache.keys().next().value!);
    cache.set(key, { size: file.size, modified: file.modified, offset: offsetStart + end + 1, points: parsed });
    points.push(...parsed);
  }
  return { activityUpdatedAt, costs: cumulativeResponseCosts(points) };
}
