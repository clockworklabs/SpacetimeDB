import { shallowRef, watch, onUnmounted } from 'vue';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { UntypedProcedureDecl } from '../sdk/procedures';
import type {
  ProcedureParamsType,
  ProcedureReturnType,
} from '../sdk/type_utils';

export function useProcedure<ProcedureDecl extends UntypedProcedureDecl>(
  procedureDef: ProcedureDecl
): (
  ...params: ProcedureParamsType<ProcedureDecl>
) => Promise<ProcedureReturnType<ProcedureDecl>> {
  const conn = useSpacetimeDB();
  const procedureName = procedureDef.accessorName;

  const queueRef = shallowRef<
    {
      params: ProcedureParamsType<ProcedureDecl>;
      resolve: (val: any) => void;
      reject: (err: unknown) => void;
    }[]
  >([]);

  const stopWatch = watch(
    () => conn.isActive,
    () => {
      const connection = conn.getConnection();
      if (!connection) return;

      const fn = (connection.procedures as any)[procedureName] as (
        ...p: ProcedureParamsType<ProcedureDecl>
      ) => Promise<ProcedureReturnType<ProcedureDecl>>;
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

  return (...params: ProcedureParamsType<ProcedureDecl>) => {
    const connection = conn.getConnection();
    if (!connection) {
      return new Promise<ProcedureReturnType<ProcedureDecl>>(
        (resolve, reject) => {
          queueRef.value.push({ params, resolve, reject });
        }
      );
    }
    const fn = (connection.procedures as any)[procedureName] as (
      ...p: ProcedureParamsType<ProcedureDecl>
    ) => Promise<ProcedureReturnType<ProcedureDecl>>;
    return fn(...params);
  };
}
