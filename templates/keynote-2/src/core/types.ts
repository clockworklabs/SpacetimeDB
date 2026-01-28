export type RunResult = {
  tps: number;
  samples: number;
  committed_txns: number | null;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  collision_ops: number;
  collision_count: number;
  collision_rate: number;
};
