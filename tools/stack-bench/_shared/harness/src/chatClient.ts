// The cross-backend contract.
//
// Every backend (SpacetimeDB, Convex, Supabase, ...) ships an *adapter* that
// implements this interface using that backend's REAL real-time client SDK.
// The behavioral scenario (scenario.ts) is written ONLY against this interface,
// so the exact same test grades every backend identically. This is what makes
// stack-bench a fair cross-backend comparison rather than N bespoke graders.

export interface ChatMessage {
  sender: string;
  text: string;
}

export interface ChatClient {
  /** Establish a real-time connection to the running app. */
  connect(): Promise<void>;

  /**
   * Subscribe to the room's message stream. `onMessage` MUST fire:
   *   - once per message already in history at subscribe time (initial sync), and
   *   - once per new message pushed in real time thereafter,
   * both in send order.
   */
  subscribe(onMessage: (msg: ChatMessage) => void): Promise<void>;

  /** Send a message as `sender`. Resolves once the backend accepts it. */
  send(sender: string, text: string): Promise<void>;

  /** Tear down the connection. */
  close(): Promise<void>;
}

/** Each adapter module must default-export a factory that builds a fresh client. */
export type ChatClientFactory = () => ChatClient;
