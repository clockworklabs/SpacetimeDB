import { shallowRef, watch, onUnmounted } from 'vue';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { UntypedReducerDef } from '../sdk/reducers';
import type { ParamsType } from '../sdk';

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => void {
  const conn = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  const queueRef = shallowRef<ParamsType<ReducerDef>[]>([]);

  const stopWatch = watch(
    () => conn.isActive,
    () => {
      const connection = conn.getConnection();
      if (!connection) return;

      const fn = (connection.reducers as any)[reducerName] as (
        ...p: ParamsType<ReducerDef>
      ) => void;
      if (queueRef.value.length) {
        const pending = queueRef.value.splice(0);
        for (const params of pending) {
          fn(...params);
        }
      }
    },
    { immediate: true }
  );

  onUnmounted(() => {
    stopWatch();
  });

  return (...params: ParamsType<ReducerDef>) => {
    const connection = conn.getConnection();
    if (!connection) {
      queueRef.value.push(params);
      return;
    }
    const fn = (connection.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;
    fn(...params);
  };
}
