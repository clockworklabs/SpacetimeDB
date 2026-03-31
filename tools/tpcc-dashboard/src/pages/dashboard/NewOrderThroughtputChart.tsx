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
import { useAppSelector } from '../../hooks';
import './NewOrderThroughputChart.css';

interface ThroughputBucketPoint {
  elapsedSec: number;
  tpmC: number;
  bucketStartMs: number;
  bucketEndMs: number;
}

function buildTpccThroughputSeries(
  transactionTimes: number[],
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

  for (const ts of transactionTimes) {
    if (ts < runStartMs || ts >= runEndMs) continue;

    const index = Math.floor((ts - runStartMs) / bucketSizeMs);
    if (index >= 0 && index < buckets.length) {
      buckets[index].count += 1;
    }
  }

  return buckets.map(bucket => ({
    elapsedSec: (bucket.bucketStartMs - runStartMs) / 1000,
    tpmC: bucket.count * (60_000 / bucketSizeMs),
    bucketStartMs: bucket.bucketStartMs,
    bucketEndMs: bucket.bucketEndMs,
  }));
}

export default function NewOrderThroughtputChart() {
  const runStartMs = useAppSelector(state => state.globalState.runStartMs);
  const runEndMs = useAppSelector(state => state.globalState.runEndMs);
  const measurementStartMs = useAppSelector(
    state => state.globalState.measureStartMs
  );
  const measurementEndMs = useAppSelector(
    state => state.globalState.measureEndMs
  );
  const data = useAppSelector(state => state.globalState.throughputData);

  const chartData = useMemo(() => {
    const bucketSizeMs = 10_000;

    return buildTpccThroughputSeries(data, runStartMs, runEndMs, bucketSizeMs);
  }, [data, runStartMs, runEndMs]);

  return (
    <div className="chart">
      <ResponsiveContainer width="100%" height={460}>
        <LineChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#142730" />
          <XAxis
            dataKey="elapsedSec"
            type="number"
            domain={[0, 'dataMax']}
            tickFormatter={(value: number) => `${Math.round(value)}s`}
            label={{
              value: 'Elapsed time',
              position: 'insideBottom',
              offset: -5,
              fill: 'var(--text-color)',
            }}
            stroke="var(--text-color)"
          />
          <YAxis
            tickFormatter={(value: number) => `${Math.round(value)}`}
            label={{
              value: 'tpmC',
              angle: -90,
              position: 'insideLeft',
              fill: 'var(--text-color)',
            }}
            stroke="var(--text-color)"
          />
          <Tooltip
            wrapperClassName="tooltip-label"
            labelFormatter={value => `Elapsed: ${value.toFixed(0)}s`}
            formatter={value => [
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              `${(value as any).toFixed(0)} tpmC`,
              'Throughput',
            ]}
          />
          <ReferenceLine
            x={(measurementStartMs - runStartMs) / 1000}
            stroke="none"
            label={{
              className: 'reference-line-label tagline-2',
              value: 'Measurement start',
              position: 'center',
              angle: -90,
            }}
          />
          <ReferenceLine
            x={(measurementEndMs - runStartMs) / 1000}
            stroke="none"
            label={{
              className: 'reference-line-label tagline-2',
              value: 'Measurement end',
              position: 'center',
              angle: -90,
            }}
          />

          <Line
            dataKey="tpmC"
            dot={false}
            isAnimationActive={false}
            stroke="#4cf490"
            strokeWidth={2}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
