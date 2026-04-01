import { useContext, useEffect } from 'react';
import { SpacetimeDBContext } from '../../context';
import {
  deleteState,
  insertState,
  upsertTxnBucket,
} from '../../features/globalState';
import LatencyDistributionChart from './LatencyDistributionChart';
import NewOrderThroughtputChart from './NewOrderThroughtputChart';
import StatsCards from './StatsCards';
import { useAppDispatch, useAppSelector } from '../../hooks';
import './DashboardPage.css';

export default function DashboardPage() {
  const conn = useContext(SpacetimeDBContext);
  const isReady = useAppSelector(state => state.globalState.isReady);
  const dispatch = useAppDispatch();

  useEffect(() => {
    console.log('App useEffect - setting up subscriptions');
    if (!conn) return;

    conn.db.state.onInsert((_, state) => {
      console.log('State inserted - dispatching insertState', state);
      dispatch(
        insertState({
          warehouseCount: Number(state.warehouseCount),
          measureStartMs: Number(state.measureStartMs),
          measureEndMs: Number(state.measureEndMs),
          runStartMs: Number(state.runStartMs),
          runEndMs: Number(state.runEndMs),
        })
      );
    });
    conn.db.state.onDelete(() => {
      console.log('State deleted - dispatching deleteState');
      dispatch(deleteState());
    });

    conn?.db.txn_bucket.onInsert((_, txn) => {
      dispatch(
        upsertTxnBucket({
          bucketStartMs: Number(txn.bucketStartMs),
          count: Number(txn.count),
        })
      );
    });
    conn?.db.txn_bucket.onUpdate((_, _old, txn) => {
      dispatch(
        upsertTxnBucket({
          bucketStartMs: Number(txn.bucketStartMs),
          count: Number(txn.count),
        })
      );
    });

    const subscription = conn
      ?.subscriptionBuilder()
      .onError(err => console.error('Subscription error:', err))
      .subscribe([
        'SELECT * FROM state',
        'SELECT * FROM txn_bucket',
        'SELECT * FROM latency_bucket',
      ]);

    return () => {
      subscription?.unsubscribe();
    };
  }, [conn, dispatch]);

  if (!isReady) {
    return <div className="heading-7">Waiting for data...</div>;
  }

  return (
    <div className="app">
      <StatsCards />
      <NewOrderThroughtputChart />
      <LatencyDistributionChart />
    </div>
  );
}
