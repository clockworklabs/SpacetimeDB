import { useContext, useEffect } from 'react';
import { SpacetimeDBContext } from './context';
import { deleteState, insertState, removeTxnBucket, upsertTxnBucket } from './features/globalState';
import { useAppDispatch, useAppSelector } from './hooks';
import NewOrderThroughtputChart from './NewOrderThroughtputChart';
import StatsCards from './StatsCards';
import './App.css';

function App() {
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

    conn.db.txn_bucket.onInsert((_, bucket) => {
      dispatch(
        upsertTxnBucket({
          bucketStartMs: Number(bucket.bucketStartMs),
          count: Number(bucket.count),
        })
      );
    });
    conn.db.txn_bucket.onUpdate((_, _oldBucket, bucket) => {
      dispatch(
        upsertTxnBucket({
          bucketStartMs: Number(bucket.bucketStartMs),
          count: Number(bucket.count),
        })
      );
    });
    conn.db.txn_bucket.onDelete((_, bucket) => {
      dispatch(
        removeTxnBucket({
          bucketStartMs: Number(bucket.bucketStartMs),
        })
      );
    });

    const subscription = conn
      .subscriptionBuilder()
      .onError(err => console.error('Subscription error:', err))
      .subscribe(['SELECT * FROM state', 'SELECT * FROM txn_bucket']);

    return () => {
      subscription.unsubscribe();
    };
  }, [conn, dispatch]);

  if (!isReady) {
    return <div className="heading-7">Waiting for data...</div>;
  }

  return (
    <div className="app">
      <StatsCards />
      <NewOrderThroughtputChart />
    </div>
  );
}

export default App;
