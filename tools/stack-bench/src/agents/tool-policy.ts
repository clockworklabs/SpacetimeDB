// Which of its own tools an agent may use beyond files and a shell: searching and reading
// the web, and starting other agents. Every agent gets the same policy, each adapter maps it
// to its own flags, and a campaign records it so its identity covers it.
export interface ToolPolicy {
  webSearch: boolean;
  subagents: boolean;
}

export const DEFAULT_TOOL_POLICY: Readonly<ToolPolicy> = Object.freeze({ webSearch: true, subagents: false });

// The controller hands the policy to each coding session as these variables and arguments.
export const TOOL_POLICY_ENVIRONMENT = { webSearch: 'STACK_BENCH_WEB_SEARCH', subagents: 'STACK_BENCH_SUBAGENTS' } as const;

export function toolPolicySwitch(enabled: boolean): 'on' | 'off' {
  return enabled ? 'on' : 'off';
}

export function parseToolPolicySwitch(value: string | undefined, fallback: boolean, at: string): boolean {
  if (value === undefined) return fallback;
  if (value === 'on') return true;
  if (value === 'off') return false;
  throw new Error(`${at} must be on or off`);
}

export function toolPolicyFromEnvironment(env: NodeJS.ProcessEnv): ToolPolicy {
  return {
    webSearch: parseToolPolicySwitch(env[TOOL_POLICY_ENVIRONMENT.webSearch], DEFAULT_TOOL_POLICY.webSearch,
      TOOL_POLICY_ENVIRONMENT.webSearch),
    subagents: parseToolPolicySwitch(env[TOOL_POLICY_ENVIRONMENT.subagents], DEFAULT_TOOL_POLICY.subagents,
      TOOL_POLICY_ENVIRONMENT.subagents),
  };
}
