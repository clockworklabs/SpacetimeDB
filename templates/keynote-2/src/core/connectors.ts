export interface BaseConnector {
  name: string;
  open(workers?: number): Promise<void>;
  close(): Promise<void>;
  getAccount(id: number): Promise<{
    id: number;
    balance: bigint;
  } | null>;
  verify(): Promise<void>;

  maxInflightPerWorker?: number;

  createWorker?(opts: { index: number; total: number }): Promise<BaseConnector>;
}

export interface SqlConnector extends BaseConnector {
  exec(sql: string, params?: unknown[]): Promise<unknown[]>;
  begin(): Promise<void>;
  commit(): Promise<void>;
  rollback(): Promise<void>;
}

export interface ReducerConnector extends BaseConnector {
  call(name: string, args?: Record<string, any>): Promise<unknown>;
}

export interface RpcConnector extends BaseConnector {
  call(name: string, args?: Record<string, any>): Promise<unknown>;
}
