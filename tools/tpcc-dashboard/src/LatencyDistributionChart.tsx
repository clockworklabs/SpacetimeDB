import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { useAppSelector } from './hooks';
import { useMemo } from 'react';

export default function LatencyDistributionChart() {
  const latencyData = useAppSelector(state => state.globalState.bucketCounts);

  const chartData = useMemo(() => {
    const sortedLatencies = Object.keys(latencyData)
      .map(key => parseInt(key))
      .sort((a, b) => a - b);

    return sortedLatencies.map(latency => ({
      latency,
      count: latencyData[latency],
    }));
  }, [latencyData]);

  return (
    <div className="chart">
      <ResponsiveContainer width="100%" height={460}>
        <LineChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis
            dataKey="latency"
            label={{
              value: 'Response Time (ms)',
              position: 'insideBottomRight',
              offset: -10,
            }}
          />
          <YAxis
            label={{
              value: 'Number of transactions',
              angle: -90,
              position: 'insideLeft',
            }}
          />
          <Tooltip />
          <Line type="monotone" dataKey="count" stroke="#8884d8" />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
