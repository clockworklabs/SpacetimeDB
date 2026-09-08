import { env_get } from 'spacetime:sys@2.3';
import type { Environment } from '../lib/environment';

/** Values are not cached: transaction and procedure reads retain host semantics. */
export const environment: Environment = Object.freeze({ get: env_get });
export type { Environment } from '../lib/environment';
