import { useMemo } from 'react';
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

export interface NewOrderThroughtputChartData {
  transactionCount: number;
  timestamp: Date;
}

interface Props {
  data: NewOrderThroughtputChartData[];
  runStartMs: number;
  runEndMs: number;
  measurementStartMs: number;
  measurementEndMs: number;
}

interface ThroughputBucketPoint {
  elapsedSec: number;
  tpmC: number;
  bucketStartMs: number;
  bucketEndMs: number;
}

function buildTpccThroughputSeries(
  samples: NewOrderThroughtputChartData[],
  runStartMs: number,
  runEndMs: number,
  bucketSizeMs: number
): ThroughputBucketPoint[] {
  if (bucketSizeMs <= 0 || runEndMs <= runStartMs) return [];

  const bucketCount = Math.ceil((runEndMs - runStartMs) / bucketSizeMs);

  const buckets = Array.from({ length: bucketCount }, (_, i) => ({
    bucketStartMs: runStartMs + i * bucketSizeMs,
    bucketEndMs: Math.min(runStartMs + (i + 1) * bucketSizeMs, runEndMs),
    count: 0,
  }));

  for (const sample of samples) {
    const ts = sample.timestamp.getTime();
    if (ts < runStartMs || ts >= runEndMs) continue;

    const index = Math.floor((ts - runStartMs) / bucketSizeMs);
    if (index >= 0 && index < buckets.length) {
      buckets[index].count += sample.transactionCount;
    }
  }

  return buckets.map(bucket => ({
    elapsedSec: (bucket.bucketStartMs - runStartMs) / 1000,
    tpmC: bucket.count * (60_000 / bucketSizeMs),
    bucketStartMs: bucket.bucketStartMs,
    bucketEndMs: bucket.bucketEndMs,
  }));
}

export default function NewOrderThroughtputChart({
  data,
  runStartMs,
  runEndMs,
  measurementStartMs,
  measurementEndMs,
}: Props) {
  const chartData = useMemo(() => {
    // const totalDurationMs = runEndMs - runStartMs;
    const bucketSizeMs = 10_000;

    return buildTpccThroughputSeries(data, runStartMs, runEndMs, bucketSizeMs);
  }, [data, runStartMs, runEndMs]);

  return (
    <ResponsiveContainer width="100%" height={320}>
      <LineChart data={chartData}>
        <CartesianGrid strokeDasharray="3 3" />
        <XAxis
          dataKey="elapsedSec"
          type="number"
          domain={[0, 'dataMax']}
          tickFormatter={(value: number) => `${Math.round(value)}s`}
          label={{
            value: 'Elapsed time',
            position: 'insideBottom',
            offset: -5,
          }}
        />
        <YAxis
          tickFormatter={(value: number) => `${Math.round(value)}`}
          label={{ value: 'tpmC', angle: -90, position: 'insideLeft' }}
          domain={[0, 'dataMax']}
        />
        <Tooltip
          labelFormatter={value => `Elapsed: ${value.toFixed(0)}s`}
          formatter={value => [
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            `${(value as any).toFixed(0)} tpmC`,
            'Throughput',
          ]}
        />
        <ReferenceLine
          x={(measurementStartMs - runStartMs) / 1000}
          label="Measurement start"
        />
        <ReferenceLine
          x={(measurementEndMs - runStartMs) / 1000}
          label="Measurement end"
        />

        <Line dataKey="tpmC" dot={false} isAnimationActive={false} />
      </LineChart>
    </ResponsiveContainer>
  );
}
