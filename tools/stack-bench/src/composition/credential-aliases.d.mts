export function validateCredentialAliases(input: unknown, at?: string): Readonly<Record<string, string>>;
export function applyCredentialAliases(value: unknown, aliases: unknown): string;
export function materializeScenarioCredentials<T>(input: T, aliases: unknown): T;
