// Client configuration
export const CONFIG = {
  SPACETIMEDB_URI:
    (import.meta as any).env?.VITE_SPACETIMEDB_URI || 'ws://localhost:3000',
  MODULE_NAME: 'chat-app',
  CLIENT_PORT: 5173,
};
