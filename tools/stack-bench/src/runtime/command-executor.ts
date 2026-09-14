export interface TextCommandOptions {
  encoding: 'utf8';
  stdio: 'pipe';
  timeout: number;
  input?: string;
}

export type TextCommandExecutor = (
  file: string,
  args: readonly string[],
  options: TextCommandOptions,
) => string;
