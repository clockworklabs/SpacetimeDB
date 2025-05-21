use convert_case::{Case, Casing};

pub const REACT_SPACETIME_REDUCER_STORE_HOOKS: &str = r#"
export function useSpacetime[name_method]Store(): [nameType][] {
  const { client } = useSpacetimeContext();
  const [name]Ref = useRef<[nameType][]>([]);

  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const updateCache = () => {
        [name]Ref.current = client.db.[name].tableCache.iter() ?? [];
        onStoreChange();
      };

      client.db.[name].onInsert(updateCache);
      client.db.[name].onDelete(updateCache);

      // Initial load
      updateCache();

      return () => {
        client.db.[name].removeOnInsert(updateCache);
        client.db.[name].removeOnDelete(updateCache);
      };
    },
    [client]
  );

  const getSnapshot = useCallback(() => [name]Ref.current, []);

  return useSyncExternalStore(subscribe, getSnapshot);
}"#;


pub const REACT_SPACETIME_CONTEXT_PROVIDER_TYPE: &str = r#"
interface SpacetimeProviderProps {
  builder: DbConnectionBuilder<
    DbConnection,
    ErrorContext,
    SubscriptionEventContext
  >;
  onConnect?(conn: DbConnection, identity: Identity, token: string): void;
  onConnectError?(ctx: ErrorContext, error: Error): void;
  onDisconnect?(ctx: ErrorContext, error?: Error | undefined): void;
}

interface SpacetimeContext {
  client: DbConnection;
  identity: Identity;
  connected: boolean;
}"#;

pub const REACT_SPACETIME_CONTEXT_PROVIDER: &str = r#"
export const SpacetimeContext = createContext<SpacetimeContext>({
  client: null,
} as unknown as SpacetimeContext);

export function SpacetimeProvider(
  props: PropsWithChildren<SpacetimeProviderProps>
) {
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [client, setClient] = useState<DbConnection>();
  const [connected, setConnected] = useState<boolean>(false);

  const onConnect = useCallback(
    (conn: DbConnection, identity: Identity, token: string) => {
      props.onConnect?.(conn, identity, token);
      setIdentity(identity);
      setConnected(true);
    },
    [props]
  );

  const onDisconnect = useCallback(
    (ctx: ErrorContext, error?: Error | undefined) => {
      props.onDisconnect?.(ctx, error);
      setConnected(false);
    },
    [props]
  );

  const onConnectError = useCallback(
    (ctx: ErrorContext, error: Error) => {
      props.onConnectError?.(ctx, error);
      setConnected(false);
    },
    [props]
  );

  useEffect(() => {
    setClient(
      props.builder
        .onConnect(onConnect)
        .onConnectError(onConnectError)
        .onDisconnect(onDisconnect)
        .build()
    );
  }, [onConnect, onConnectError, onDisconnect, props.builder]);

  const providerValue = useMemo(
    () =>
      ({
        client,
        identity,
        connected,
      }) as SpacetimeContext,
    [client, connected, identity]
  );

  return (
    <SpacetimeContext.Provider value={providerValue}>
      {props.children}
    </SpacetimeContext.Provider>
  );
}"#;

pub const REACT_SPACETIME_CONTEXT_HOOKS: &str = r#"
export function useSpacetimeContext() {
  return useContext(SpacetimeContext);
}"#;

pub fn replace_reducer_store_hooks(template: &str, name: &str, name_type: &str) -> String {
  template
      .replace("[name_method]", &name.to_case(Case::Pascal))
      .replace("[name]", name)
      .replace("[nameType]", name_type)
}

pub fn import_from(named_imports: &[&str], path: &str) -> String {
  format!("import {{ {} }} from '{}';", named_imports.join(", "), path)
}

pub fn import_from_types(named_imports: &[&str], path: &str) -> String {
  format!("import type {{ {} }} from '{}';", named_imports.join(", "), path)
}