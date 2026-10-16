export class S3Error extends Error {}
export interface Credentials {
  accessKeyId: string;
  secretAccessKey: string;
  sessionToken?: string;
  /** Unix milliseconds. Required with a session token. */
  expiresAt?: number;
}
export interface S3Config {
  bucket: string;
  region: string;
  /** Exclusive prefix for this installation, with a trailing slash. */
  prefix: string;
  accountId: string;
  maxFileBytes?: number;
  maxFilesPerOwner?: number;
  maxBytesPerOwner?: number;
  uploadSeconds?: number;
  downloadSeconds?: number;
  recoverySeconds?: number;
  receiptSeconds?: number;
  sweepSeconds?: number;
  httpTimeoutSeconds?: number;
}
export function resolveConfig(input: S3Config) {
  const config = {
    ...input,
    maxFileBytes: input.maxFileBytes ?? 100 * 1024 * 1024,
    maxFilesPerOwner: input.maxFilesPerOwner ?? 100,
    maxBytesPerOwner: input.maxBytesPerOwner ?? 1024 * 1024 * 1024,
    uploadSeconds: input.uploadSeconds ?? 300,
    downloadSeconds: input.downloadSeconds ?? 60,
    recoverySeconds: input.recoverySeconds ?? 3600,
    receiptSeconds: input.receiptSeconds ?? 86400,
    sweepSeconds: input.sweepSeconds ?? 300,
    httpTimeoutSeconds: input.httpTimeoutSeconds ?? 5,
  };
  if (
    !/^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$/.test(config.bucket) ||
    /--(?:ol-s3|x-s3|table-s3)$|-s3alias$/.test(config.bucket) ||
    !/^(us|eu|ap|sa|ca|me|af|il|mx)-(?:[a-z]+-)?[a-z]+-\d+$/.test(
      config.region
    ) ||
    config.region.startsWith('us-gov-') ||
    !/^\d{12}$/.test(config.accountId) ||
    !/^(?:[a-zA-Z0-9_-]+\/)+$/.test(config.prefix) ||
    config.prefix.length > 128
  )
    throw new S3Error('s3.invalid_config');
  const limits = [
    [config.maxFileBytes, 0, 1024 * 1024 * 1024],
    [config.maxFilesPerOwner, 1, 10000],
    [config.maxBytesPerOwner, config.maxFileBytes, Number.MAX_SAFE_INTEGER],
    [config.uploadSeconds, 1, 3600],
    [config.downloadSeconds, 1, 3600],
    [config.recoverySeconds, 1, 86400],
    [config.receiptSeconds, 1, 604800],
    [config.sweepSeconds, 1, 86400],
    [config.httpTimeoutSeconds, 1, 60],
  ];
  if (
    limits.some(
      ([value, min, max]) =>
        !Number.isSafeInteger(value) || value < min || value > max
    )
  )
    throw new S3Error('s3.invalid_config');
  return config;
}
export type ResolvedConfig = ReturnType<typeof resolveConfig>;
export function credentialExpiry(credentials: Credentials, now: number) {
  if (
    !credentials.accessKeyId ||
    !credentials.secretAccessKey ||
    (credentials.sessionToken && credentials.expiresAt === undefined) ||
    (credentials.expiresAt !== undefined &&
      !Number.isSafeInteger(credentials.expiresAt))
  )
    throw new S3Error('s3.invalid_credentials');
  const expires =
    credentials.expiresAt === undefined
      ? Infinity
      : credentials.expiresAt - 30000;
  // Every grant needs at least one usable second.
  if (expires < now + 1000) throw new S3Error('s3.credentials_expiring');
  return expires;
}
