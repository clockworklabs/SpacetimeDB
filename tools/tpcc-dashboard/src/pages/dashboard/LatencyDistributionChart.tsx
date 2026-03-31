import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { useAppSelector } from '../../hooks';
import { useMemo } from 'react';

export default function LatencyDistributionChart() {
  const latencyData = useAppSelector(state => state.globalState.latencyData);

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
          <CartesianGrid strokeDasharray="3 3" stroke="#142730" />
          <XAxis
            domain={[0, 'dataMax']}
            dataKey="latency"
            type="number"
            label={{
              value: 'Response Time (ms)',
              position: 'insideBottom',
              offset: -5,
              fill: 'var(--text-color)',
            }}
            stroke="var(--text-color)"
          />
          <YAxis
            type="number"
            label={{
              value: 'Number of transactions',
              angle: -90,
              position: 'inside',
              fill: 'var(--text-color)',
            }}
            stroke="var(--text-color)"
          />
          <Tooltip
            wrapperClassName="tooltip-label"
            formatter={value => `${value} transactions`}
            labelFormatter={value => `Latency: ${value} ms`}
          />
          <Line
            dataKey="count"
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
