import { useAppSelector } from './hooks';
import {
  ClockIcon,
  ConnectIcon,
  DataIcon,
  PercentIcon,
  RefreshIcon,
  SchemaIcon,
  UploadIcon,
} from './Icons';
import './StatsCards.css';

function getTpmC(
  measureStartMs: number,
  bucketCounts: Record<number, number>
): number {
  const measuredBucketStarts = Object.keys(bucketCounts)
    .map(Number)
    .filter(bucketStartMs => bucketStartMs >= measureStartMs)
    .sort((a, b) => a - b);

  if (measuredBucketStarts.length === 0) {
    return 0;
  }

  const firstBucketStartMs = measuredBucketStarts[0];
  const latestBucketStartMs =
    measuredBucketStarts[measuredBucketStarts.length - 1];
  const totalMeasuredTransactions = measuredBucketStarts.reduce(
    (sum, bucketStartMs) => sum + (bucketCounts[bucketStartMs] ?? 0),
    0
  );
  const elapsedTimeSec =
    (latestBucketStartMs + 1_000 - firstBucketStartMs) / 1000;

  if (elapsedTimeSec <= 0) {
    return 0;
  }

  return Math.trunc((totalMeasuredTransactions / elapsedTimeSec) * 60);
}

function StatCard({
  icon,
  label,
  value,
  unit,
}: {
  icon: React.ReactNode;
  label: string;
  value: string | number;
  unit?: string;
}) {
  return (
    <div className="card">
      {icon}
      <p className="heading-7">{label}</p>
      <div>
        <p className="value-1">{value}</p>
        {unit && <p className="value-3">{unit}</p>}
      </div>
    </div>
  );
}

export default function StatsCards() {
  const warehouses = useAppSelector(state => state.globalState.warehouses);
  const measureStartMs = useAppSelector(
    state => state.globalState.measureStartMs
  );
  const measureEndMs = useAppSelector(state => state.globalState.measureEndMs);
  const totalTransactionCount = useAppSelector(
    state => state.globalState.totalTransactionCount
  );
  const measuredTransactionCount = useAppSelector(
    state => state.globalState.measuredTransactionCount
  );
  const bucketCounts = useAppSelector(state => state.globalState.bucketCounts);

  const tpmC = getTpmC(measureStartMs, bucketCounts);
  const theoreticalMaxThroughput = warehouses * 12.86;

  return (
    <div className="cards">
      <StatCard
        icon={<ClockIcon />}
        label="Measured Duration"
        value={((measureEndMs - measureStartMs) / 1000 / 60).toFixed(2)}
        unit="minutes"
      />
      <StatCard icon={<SchemaIcon />} label="Warehouses" value={warehouses} />
      <StatCard
        icon={<UploadIcon />}
        label="Max. Theorical Throughput"
        value={theoreticalMaxThroughput}
        unit="tpmC"
      />
      <StatCard
        icon={<PercentIcon />}
        label="% Max. Theorical Throughput"
        value={
          theoreticalMaxThroughput <= 0
            ? 'N/A'
            : ((tpmC / theoreticalMaxThroughput) * 100).toFixed(2) + '%'
        }
      />
      <StatCard
        icon={<RefreshIcon />}
        label="Total Transactions"
        value={totalTransactionCount}
      />
      <StatCard
        icon={<DataIcon />}
        label="Measured Transactions"
        value={measuredTransactionCount}
      />
      <StatCard
        icon={<ConnectIcon />}
        label="MQTh"
        value={tpmC + ' tpmC'}
      />
    </div>
  );
}
