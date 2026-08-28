export interface PreflightRequest {
  json?: boolean;
  report?: string;
  [key: string]: unknown;
}

export interface PreflightReport {
  ok: boolean;
  [key: string]: unknown;
}

export function parsePreflightArgs(argv: string[]): PreflightRequest;
export function runPreflight(request: PreflightRequest): PreflightReport;
export function writePreflightReport(path: string, report: PreflightReport): void;
export function printPreflightReport(report: PreflightReport): void;
