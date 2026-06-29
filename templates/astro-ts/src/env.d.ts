interface ImportMetaEnv {
  readonly SPACETIMEDB_HOST?: string;
  readonly SPACETIMEDB_DB_NAME?: string;
  readonly PUBLIC_SPACETIMEDB_HOST?: string;
  readonly PUBLIC_SPACETIMEDB_DB_NAME?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
