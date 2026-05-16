// Simple latency aggregator: stores raw samples in ms, computes percentiles on demand.
// For our scenarios (≤ a few hundred thousand samples) this is plenty.

export class LatencyHistogram {
  private samples: number[] = [];

  record(ms: number): void {
    this.samples.push(ms);
  }

  count(): number {
    return this.samples.length;
  }

  summary(): LatencySummary {
    if (this.samples.length === 0) {
      return { count: 0, min: 0, max: 0, mean: 0, p50: 0, p95: 0, p99: 0, p999: 0 };
    }
    const sorted = [...this.samples].sort((a, b) => a - b);
    const pct = (p: number): number => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))]!;
    const sum = sorted.reduce((a, b) => a + b, 0);
    return {
      count: sorted.length,
      min: sorted[0]!,
      max: sorted[sorted.length - 1]!,
      mean: sum / sorted.length,
      p50: pct(0.50),
      p95: pct(0.95),
      p99: pct(0.99),
      p999: pct(0.999),
    };
  }
}

export interface LatencySummary {
  count: number;
  min: number;
  max: number;
  mean: number;
  p50: number;
  p95: number;
  p99: number;
  p999: number;
}

export interface ScenarioResult {
  scenario: string;
  backend: 'postgres' | 'spacetime';
  startedAt: string;
  durationSec: number;
  writers: number;
  sent: number;
  received: number;
  errors: number;
  msgsPerSec: number;
  ackLatencyMs: LatencySummary;
  fanoutLatencyMs: LatencySummary;
  notes?: string;
}

// Encode/decode timestamp + sequence into the message text so the listener can compute fan-out latency.
const MARKER = '__bench:';
export function stampMessage(seq: number): string {
  return `${MARKER}${process.hrtime.bigint().toString()}:${seq}:hello`;
}

export function parseStamp(text: string): { sentNs: bigint; seq: number } | null {
  if (!text.startsWith(MARKER)) return null;
  const rest = text.slice(MARKER.length);
  const parts = rest.split(':');
  if (parts.length < 2) return null;
  try {
    return { sentNs: BigInt(parts[0]!), seq: parseInt(parts[1]!) };
  } catch {
    return null;
  }
}

export function nsToMs(deltaNs: bigint): number {
  return Number(deltaNs) / 1_000_000;
}
