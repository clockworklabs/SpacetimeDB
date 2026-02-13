import { shallowRef, watch, onUnmounted } from 'vue';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { InferTypeOfRow } from '../lib/type_builders';
import type { UntypedReducerDef } from '../sdk/reducers';
import type { Prettify } from '../lib/type_util';

type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

type ParamsType<R extends UntypedReducerDef> = MaybeParams<
  Prettify<InferTypeOfRow<R['params']>>
>;

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void> {
  const conn = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  const queueRef = shallowRef<
    {
      params: ParamsType<ReducerDef>;
      resolve: () => void;
      reject: (err: unknown) => void;
    }[]
  >([]);

  const stopWatch = watch(
    () => conn.isActive,
    () => {
      const connection = conn.getConnection();
      if (!connection) return;

      const fn = (connection.reducers as any)[reducerName] as (
        ...p: ParamsType<ReducerDef>
      ) => Promise<void>;
      if (queueRef.value.length) {
        const pending = queueRef.value.splice(0);
        for (const item of pending) {
          fn(...item.params).then(item.resolve, item.reject);
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
      return new Promise<void>((resolve, reject) => {
        queueRef.value.push({ params, resolve, reject });
      });
    }
    const fn = (connection.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => Promise<void>;
    return fn(...params);
  };
}
