export const SPACETIME_VERSIONS = {
  SDK: "^1.3.1",
  RUNTIME: "1.3.*",
  CLI: "1.3",
} as const;

export const SERVER_CONFIG = {
  LOCAL_PORT: 3000,
  MAINCLOUD_URI: "wss://maincloud.spacetimedb.com",
  LOCAL_URI: "ws://localhost:3000",
} as const;
