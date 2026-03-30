import { useContext, useEffect } from 'react';
import { SpacetimeDBContext } from './context';
import {
  deleteState,
  insertState,
  throughputStateUpdate,
} from './features/globalState';
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

    conn?.db.txn.onInsert((_, txn) => {
      dispatch(
        throughputStateUpdate({
          id: String(txn.id),
          measurementTimeMs: Number(txn.measurementTimeMs),
          latencyMs: Number(txn.latencyMs),
        })
      );
    });

    const subscription = conn
      ?.subscriptionBuilder()
      .onError(err => console.error('Subscription error:', err))
      .subscribe(['SELECT * FROM state', 'SELECT * FROM txn']);

    return () => {
      subscription?.unsubscribe();
    };
  }, [conn, dispatch]);

  if (!isReady) {
    return <div>Waiting for data...</div>;
  }

  return (
    <div className="app">
      <StatsCards />
      <NewOrderThroughtputChart />
    </div>
  );
}

export default App;
