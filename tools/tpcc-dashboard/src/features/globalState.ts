import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

export interface GlobalState {
  isReady: boolean;
  warehouses: number;
  measureStartMs: number;
  measureEndMs: number;
  runStartMs: number;
  runEndMs: number;
  totalTransactionCount: number;
  measuredTransactionCount: number;
  transactionBuckets: Record<number, number>;
  latencyBuckets: Record<number, number>;
}

function createInitialState(): GlobalState {
  return {
    isReady: false,
    warehouses: 0,
    measureStartMs: 0,
    measureEndMs: 0,
    runStartMs: 0,
    runEndMs: 0,
    totalTransactionCount: 0,
    measuredTransactionCount: 0,
    transactionBuckets: {},
    latencyBuckets: {},
  };
}

function recalculateCounts(state: GlobalState) {
  let totalTransactionCount = 0;
  let measuredTransactionCount = 0;

  for (const [bucketStartMs, count] of Object.entries(
    state.transactionBuckets
  )) {
    const bucketCount = Number(count);
    totalTransactionCount += bucketCount;

    if (Number(bucketStartMs) >= state.measureStartMs) {
      measuredTransactionCount += bucketCount;
    }
  }

  state.totalTransactionCount = totalTransactionCount;
  state.measuredTransactionCount = measuredTransactionCount;
}

export const globalStateSlice = createSlice({
  name: 'globalState',
  initialState: createInitialState(),
  reducers: {
    insertState: (
      state,
      action: PayloadAction<{
        warehouseCount: number;
        measureStartMs: number;
        measureEndMs: number;
        runStartMs: number;
        runEndMs: number;
      }>
    ) => {
      console.log('State inserted, updating global state');
      const payload = action.payload;
      state.isReady = true;
      state.warehouses = payload.warehouseCount;
      state.measureStartMs = payload.measureStartMs;
      state.measureEndMs = payload.measureEndMs;
      state.runStartMs = payload.runStartMs;
      state.runEndMs = payload.runEndMs;
      state.totalTransactionCount = 0;
      state.measuredTransactionCount = 0;
      state.transactionBuckets = {};
      state.latencyBuckets = {};
    },
    deleteState: () => {
      console.log('State deleted, resetting to initial state');
      return createInitialState();
    },
    upsertTxnBucket: (
      state,
      action: PayloadAction<{
        bucketStartMs: number;
        count: number;
      }>
    ) => {
      const payload = action.payload;
      state.transactionBuckets[payload.bucketStartMs] = payload.count;
      recalculateCounts(state);
    },
  },
});

export const { insertState, deleteState, upsertTxnBucket } =
  globalStateSlice.actions;

export default globalStateSlice.reducer;
