import { shallowRef, watch, onUnmounted } from 'vue';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { UntypedProcedureDef } from '../sdk/procedures';
import type {
    ProcedureParamsType,
    ProcedureReturnType,
} from '../sdk/type_utils';

export function useProcedure<ProcedureDef extends UntypedProcedureDef>(
    procedureDef: ProcedureDef
): (...params: ProcedureParamsType<ProcedureDef>) => Promise<ProcedureReturnType<ProcedureDef>> {
    const conn = useSpacetimeDB();
    const procedureName = procedureDef.accessorName;

    const queueRef = shallowRef<
        {
            params: ProcedureParamsType<ProcedureDef>;
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
                ...p: ProcedureParamsType<ProcedureDef>
            ) => Promise<ProcedureReturnType<ProcedureDef>>;
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

    return (...params: ProcedureParamsType<ProcedureDef>) => {
        const connection = conn.getConnection();
        if (!connection) {
            return new Promise<ProcedureReturnType<ProcedureDef>>((resolve, reject) => {
                queueRef.value.push({ params, resolve, reject });
            });
        }
        const fn = (connection.procedures as any)[procedureName] as (
            ...p: ProcedureParamsType<ProcedureDef>
        ) => Promise<ProcedureReturnType<ProcedureDef>>;
        return fn(...params);
    };
}
