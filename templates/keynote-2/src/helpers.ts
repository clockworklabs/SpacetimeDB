export function poolMax(
  workers: number | undefined,
  envName: string,
  fallbackEnvMax: number,
  defaultWorkers = 1,
): number {
  const requested =
    workers && Number.isFinite(workers) && workers > 0
      ? workers
      : defaultWorkers;

  const raw = process.env[envName];
  const parsed = raw !== undefined ? Number(raw) : NaN;
  const envMax =
    Number.isFinite(parsed) && parsed > 0 ? parsed : fallbackEnvMax;

  return Math.min(requested, envMax);
}

export function poolMaxFromEnv(fallback = 1000): number {
  const raw = process.env.MAX_POOL;
  const n = raw !== undefined ? Number(raw) : NaN;
  return Number.isFinite(n) && n > 0 ? n : fallback;
}
