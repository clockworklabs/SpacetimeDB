export type TimeSeriesPoint = {
  tSec: number;
  tps: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  samples: number;
};

export type RunResult = {
  tps: number;
  samples: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  collision_ops: number;
  collision_count: number;
  collision_rate: number;
  timeSeries: TimeSeriesPoint[];
};
