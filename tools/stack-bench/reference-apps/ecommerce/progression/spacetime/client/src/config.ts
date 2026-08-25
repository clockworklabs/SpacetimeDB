function required(name: string, value: string | undefined): string {
  if (!value) throw new Error(`Missing required ${name}`);
  return value;
}

export const MODULE_NAME = required('VITE_MODULE_NAME', import.meta.env.VITE_MODULE_NAME);
export const SPACETIMEDB_URI = required('VITE_SPACETIMEDB_URI', import.meta.env.VITE_SPACETIMEDB_URI);
