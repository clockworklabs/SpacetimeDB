/* eslint-disable react-hooks/purity */
import { useContext, useEffect, useState } from 'react';
import { SpacetimeDBContext } from './context';
import type { NewOrderThroughtputChartData } from './NewOrderThroughtputChart';
import NewOrderThroughtputChart from './NewOrderThroughtputChart';
import type { State } from './module_bindings/types';

function getTpmC(state: State): number | null {
  const measureStartDate = new Date(Number(state.measureStartMs));
  const measureEndDate = new Date(Number(state.measureEndMs));
  // If we are in warmup, return "Warmup in progress..."
  if (Date.now() < measureStartDate.getTime()) {
    return null;
  }

  // If the run is in progress we calculate the ellapsed time based on the current time,
  if (Date.now() < measureEndDate.getTime()) {
    const ellapsedTimeSec = (Date.now() - measureStartDate.getTime()) / 1000;
    const tpmC = (Number(state.orderCount) / ellapsedTimeSec) * 60;
    return Math.trunc(tpmC);
  }

  // otherwise we calculate it based on the measure start and end date
  const ellapsedTimeSec =
    (measureEndDate.getTime() - measureStartDate.getTime()) / 1000;
  const tpmC = (Number(state.orderCount) / ellapsedTimeSec) * 60;
  return Math.trunc(tpmC);
}

function App() {
  const conn = useContext(SpacetimeDBContext);
  const [state, setState] = useState<State | null>(null);
  const [measuredTransactionCount, setMeasuredTransactionCount] = useState(0);
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

      if (Date.now() >= Number(state.measureStartMs)) {
        setMeasuredTransactionCount(
          prev => prev + Number(state.orderCount - old.orderCount)
        );
      }

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

  const tpmC = getTpmC(state);

  return (
    <>
      <p>
        Measured duration: {measureStartDate.toLocaleTimeString()} -{' '}
        {measureEndDate.toLocaleTimeString()} (
        {Math.round(
          (measureEndDate.getTime() - measureStartDate.getTime()) / 60000
        )}{' '}
        minutes)
      </p>
      <p>Total transactions: {state.orderCount}</p>
      <p>Measured transactions: {measuredTransactionCount}</p>
      <p>
        MQTh:{' '}
        {tpmC === null ? 'Warmup in progress...' : Math.trunc(tpmC) + ' tpmC'}
      </p>
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
