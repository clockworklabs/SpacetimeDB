import { useSpacetimeDB, useTable } from '../../../src/react';
import { DbConnection, Counter } from '../module_bindings';

export default function CounterPage() {
  const stdb = useSpacetimeDB<DbConnection>();
  const { rows: counter } = useTable<DbConnection, Counter>('counter');

  console.log('Rendering CounterPage, current counter:', counter);

  return (
    <>
      <h1>Counter</h1>
      <div className="card">
        <button onClick={() => stdb.reducers.incrementCounter()}>
          count is {counter[0]?.count}
        </button>
        <p>
          Click above to increment the count, click below to clear the count.
        </p>
        <button onClick={() => stdb.reducers.clearCounter()}>
          clear count
        </button>
      </div>
    </>
  );
}
