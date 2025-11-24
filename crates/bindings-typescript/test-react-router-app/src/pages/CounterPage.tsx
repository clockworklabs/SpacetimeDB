import { useReducer, useTable } from '../../../src/react';
import { tables, reducers } from '../module_bindings';

export default function CounterPage() {
  const [counter] = useTable(tables.counter);
  const incrementCounter = useReducer(reducers.incrementCounter);
  const clearCounter = useReducer(reducers.clearCounter);

  console.log('Rendering CounterPage, current counter:', counter);

  return (
    <>
      <h1>Counter</h1>
      <div className="card">
        <button onClick={() => incrementCounter()}>
          count is {counter[0]?.count}
        </button>
        <p>
          Click above to increment the count, click below to clear the count.
        </p>
        <button onClick={() => clearCounter()}>clear count</button>
      </div>
    </>
  );
}
