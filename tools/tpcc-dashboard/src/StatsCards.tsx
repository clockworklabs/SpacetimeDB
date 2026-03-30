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
  measureEndMs: number,
  measuredTransactionCount: number
): number | null {
  const nowMs = Date.now();

  if (measureStartMs <= 0 || measureEndMs <= measureStartMs) {
    return null;
  }

  if (nowMs < measureStartMs) {
    return null;
  }

  const effectiveEndMs = Math.min(nowMs, measureEndMs);
  const elapsedTimeSec = (effectiveEndMs - measureStartMs) / 1000;

  if (elapsedTimeSec <= 0) {
    return null;
  }

  const tpmC = (measuredTransactionCount / elapsedTimeSec) * 60;
  return Math.trunc(tpmC);
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

  const tpmC = getTpmC(measureStartMs, measureEndMs, measuredTransactionCount);

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
        value={warehouses * 12.86}
        unit="tpmC"
      />
      <StatCard
        icon={<PercentIcon />}
        label="% Max. Theorical Throughput"
        value={
          tpmC === null
            ? 'N/A'
            : ((tpmC / (warehouses * 12.86)) * 100).toFixed(2) + '%'
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
        value={
          tpmC === null ? 'Warmup in progress...' : Math.trunc(tpmC) + ' tpmC'
        }
      />
    </div>
  );
}
