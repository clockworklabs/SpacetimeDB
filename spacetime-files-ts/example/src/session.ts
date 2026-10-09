export interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

const STDB_TOKEN_KEY = 'vault:auth-token';

export function loadStdbToken(): string | undefined {
  try {
    return localStorage.getItem(STDB_TOKEN_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

export function saveStdbToken(token: string | undefined): void {
  try {
    if (token) localStorage.setItem(STDB_TOKEN_KEY, token);
  } catch {
    // The connection keeps the token in memory when storage is unavailable.
  }
}

export function clearStdbToken(): void {
  try {
    localStorage.removeItem(STDB_TOKEN_KEY);
  } catch {
    // Storage is optional.
  }
}
