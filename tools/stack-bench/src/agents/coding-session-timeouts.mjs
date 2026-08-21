// A coding turn can include dependency installation, implementation, and local
// verification. Keep it bounded, but do not cut it off at the old 55-minute
// limit while the provider is still making progress.
export const CODING_SESSION_TIMEOUT_MS = 120 * 60_000;
export const AGENT_PROCESS_TIMEOUT_MS = CODING_SESSION_TIMEOUT_MS + 3 * 60_000;
