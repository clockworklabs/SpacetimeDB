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
  /// Time in ms when the transaction was measured
  throughputData: number[];
}

const initialState: GlobalState = {
  isReady: false,
  warehouses: 0,
  measureStartMs: 0,
  measureEndMs: 0,
  runStartMs: 0,
  runEndMs: 0,
  totalTransactionCount: 0,
  measuredTransactionCount: 0,
  throughputData: [],
};

export const globalStateSlice = createSlice({
  name: 'globalState',
  initialState,
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
      state.throughputData = [];
    },
    deleteState: state => {
      console.log('State deleted, resetting to initial state');
      state.isReady = false;
    },
    throughputStateUpdate: (
      state,
      action: PayloadAction<{
        id: string;
        measurementTimeMs: number;
        latencyMs: number;
      }>
    ) => {
      const payload = action.payload;
      state.totalTransactionCount += 1;
      if (Number(payload.measurementTimeMs) >= state.measureStartMs) {
        // Each update here is a single transaction, so we can just increment the count by one.
        state.measuredTransactionCount += 1;
      }

      state.throughputData.push(Number(payload.measurementTimeMs));
    },
  },
});

export const { insertState, deleteState, throughputStateUpdate } =
  globalStateSlice.actions;

export default globalStateSlice.reducer;
