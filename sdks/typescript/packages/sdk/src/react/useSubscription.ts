import { useEffect, useMemo, useRef, useCallback } from "react";
import type { DbConnection } from "../module_bindings";
import { useSpacetimeDB } from "./useSpacetimeDB";

type UseSubscriptionCallbacks = {
  onApplied?: () => void;
  onError?: (error: Error) => void;
  onUnsubscribed?: () => void;
};

export function useSubscription(query: string, callbacks?: UseSubscriptionCallbacks) {
  const { client } = useSpacetimeDB<DbConnection>();

  const cbRef = useRef<UseSubscriptionCallbacks | undefined>(callbacks);
  cbRef.current = callbacks;

  const cancelRef = useRef<(() => Promise<void> | void) | null>(null);

  const cancel = useCallback(() => {
    return cancelRef.current?.();
  }, []);

  useEffect(() => {
    const subscription = client
      .subscriptionBuilder()
      .onApplied(() => cbRef.current?.onApplied?.())
      .onError((e) => cbRef.current?.onError?.(e as any))
      .subscribe(query);

    cancelRef.current = () => {
      subscription.unsubscribe();
    };

    return () => {
      const fn = cancelRef.current;
      cancelRef.current = null;
      fn?.();
    };
  }, [query, client]);

  return useMemo(() => ({ cancel }), [cancel]);
}
