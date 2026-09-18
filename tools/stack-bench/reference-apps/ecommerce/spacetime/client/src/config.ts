function required(name: string, value: string | undefined): string {
  if (!value) throw new Error(`Missing required ${name}`);
  return value;
}

export const MODULE_NAME = required('VITE_MODULE_NAME', import.meta.env.VITE_MODULE_NAME);
export const SPACETIMEDB_URI = required('VITE_SPACETIMEDB_URI', import.meta.env.VITE_SPACETIMEDB_URI);
export const OIDC_ISSUER = required('VITE_OIDC_ISSUER', import.meta.env.VITE_OIDC_ISSUER);
export const OIDC_CLIENT_ID = required('VITE_OIDC_CLIENT_ID', import.meta.env.VITE_OIDC_CLIENT_ID);
export const OIDC_REDIRECT_URI = required('VITE_OIDC_REDIRECT_URI', import.meta.env.VITE_OIDC_REDIRECT_URI);
