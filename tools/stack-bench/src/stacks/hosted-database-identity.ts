export const POSTGRES_APPLICATION_IDENTITY = Object.freeze({
  user: 'appuser',
  password: 'local-app-password',
  defaultDatabase: 'app',
});
import { createHmac } from 'node:crypto';

export function attemptDatabaseIdentity(ownershipToken: string) {
  if (!ownershipToken) throw new Error('database credentials require private lease authority');
  const secret = (purpose: string) => createHmac('sha256', ownershipToken).update(purpose).digest('hex');
  return { user: 'appuser', password: secret('application-database'), adminPassword: secret('database-administration') };
}

export function attemptDatabaseUrl({ backend, database, ownershipToken }: {
  backend: string; database: string; ownershipToken: string;
}): string {
  const { user, password } = attemptDatabaseIdentity(ownershipToken);
  if (backend === 'postgres') return `postgresql://${user}:${password}@127.0.0.1:5432/${database}`;
  if (backend === 'mongodb') return `mongodb://${user}:${password}@127.0.0.1:27017/${database}?authSource=${database}`;
  throw new Error(`no hosted database URL for ${backend}`);
}
