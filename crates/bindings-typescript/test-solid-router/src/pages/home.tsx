import { useReducer, useTable } from '../../../src/solid';
import { tables, reducers } from '../module_bindings';

export default function Home() {
  const [counter] = useTable(tables.counter);
  const incrementCounter = useReducer(reducers.incrementCounter);
  const clearCounter = useReducer(reducers.clearCounter);

  console.log('Rendering CounterPage, current counter:', counter);

  return (
    <section class="bg-gray-100 text-gray-700 p-8">
      <h1 class="text-2xl font-bold">Counter</h1>

      <div class="flex items-center space-x-4 mt-4">
        <button
          type="button"
          class="border rounded-lg px-4 py-2 border-gray-900 bg-white hover:bg-gray-50"
          onClick={() => incrementCounter()}
        >
          count is {counter[0]?.count}
        </button>

        <button
          type="button"
          class="border rounded-lg px-4 py-2 border-gray-400 bg-white hover:bg-gray-50"
          onClick={() => clearCounter()}
        >
          clear count
        </button>
      </div>

      <p class="mt-4 text-sm text-gray-500">
        Click above to increment the count, click below to clear the count.
      </p>
    </section>
  );
}
