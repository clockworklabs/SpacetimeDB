export const DEFAULT_TARGET_DIR = "my-spacetime-app";

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

export const TIMEOUTS = {
  COMMAND_TIMEOUT: 10000,
  SERVER_START_DELAY: 3000,
} as const;

export const SPACETIME_SDK_PACKAGE = "spacetimedb";
