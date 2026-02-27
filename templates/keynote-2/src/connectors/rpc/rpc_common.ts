export type RpcRequest = {
  name?: string;
  args?: Record<string, unknown>;
};

export type RpcResponse =
  | { ok: true; result?: unknown }
  | { ok: false; error: string };
