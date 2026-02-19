/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SPACETIMEDB_HOST: string;
  readonly VITE_SPACETIMEDB_DB_NAME: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
