export function sanitiseDiagnostic(detail?: unknown, limit?: number): string;
export function sanitiseConsoleError(detail?: unknown): string;
export function humaniseDiagnostic(detail?: unknown): string;
export function redactCredentials(value: string): string;
