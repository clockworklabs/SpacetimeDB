/* eslint-disable react-hooks/purity */
import { useContext, useEffect, useState } from 'react';
import { SpacetimeDBContext } from './context';
import type { NewOrderThroughtputChartData } from './NewOrderThroughtputChart';
import NewOrderThroughtputChart from './NewOrderThroughtputChart';
import type { State } from './module_bindings/types';

function App() {
  const conn = useContext(SpacetimeDBContext);
  const [state, setState] = useState<State | null>(null);
  const [throughputData, setThroughputData] = useState<
    NewOrderThroughtputChartData[]
  >([]);

  useEffect(() => {
    conn?.db.state.onInsert((_, state) => {
      setState(state);
      setThroughputData([]);
    });
    conn?.db.state.onUpdate((_, old, state) => {
      setState(state);

      setThroughputData(prevData => {
        const next = [
          ...prevData,
          {
            transactionCount: Number(state.orderCount - old.orderCount),
            timestamp: new Date(Number(state.measurementTimeMs)),
          },
        ];

        return next;
      });
    });

    conn?.db.state.onDelete(() => {
      setState(null);
      setThroughputData([]);
    });

    const subscription = conn
      ?.subscriptionBuilder()
      .onError(err => console.error('Subscription error:', err))
      .subscribe('SELECT * FROM state');

    return () => {
      subscription?.unsubscribe();
    };
  }, [conn]);

  if (!state) {
    return <div>Waiting for data...</div>;
  }

  const measureStartDate = new Date(Number(state.measureStartMs));
  const measureEndDate = new Date(Number(state.measureEndMs));

  // If the is in progress we calculate the ellapsed time based on the current time,
  // otherwise we calculate it based on the measure end date
  const ellapsedTimeSec =
    Date.now() > measureEndDate.getTime()
      ? (measureEndDate.getTime() - measureStartDate.getTime()) / 1000
      : (Date.now() - measureStartDate.getTime()) / 1000;
  const tpmC = (Number(state.orderCount) / ellapsedTimeSec) * 60;

  return (
    <>
      <p>measureStartMs: {measureStartDate.toLocaleTimeString()}</p>
      <p>measureEndMs: {measureEndDate.toLocaleTimeString()}</p>
      <p>total transactions: {state.orderCount}</p>
      <p>MQTh: {Math.trunc(tpmC)} tpmC</p>
      <NewOrderThroughtputChart
        data={throughputData}
        runStartMs={Number(state.runStartMs)}
        runEndMs={Number(state.runEndMs)}
        measurementStartMs={Number(state.measureStartMs)}
        measurementEndMs={Number(state.measureEndMs)}
      />
      <button onClick={() => conn?.reducers.clearState({})}>Clear state</button>
    </>
  );
}

export default App;
