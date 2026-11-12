// Which of its own tools an agent may use beyond files and a shell: searching and reading
// the web, and starting other agents. Every agent gets the same policy, each adapter maps it
// to its own flags, and a campaign records it so its identity covers it.
export interface ToolPolicy {
  webSearch: boolean;
  subagents: boolean;
}

// Off by default, as every agent ran in StackBench 1.1; turning a tool on is its own condition.
export const DEFAULT_TOOL_POLICY: Readonly<ToolPolicy> = Object.freeze({ webSearch: false, subagents: false });

// Why a policy cannot run, or null. A policy that cannot be honoured is refused rather than
// recorded for a run that differs from it. Subagents are refused for every agent: the
// contamination audit reads each agent's own event log, and no fixture yet shows a subagent's
// shell calls reaching it. Claude Code runs with `--bare`, which offers only Bash, Edit and Read
// (checked on Claude Code 2.1.284 in the build image), so it has no web search either.
export function unavailableToolsReason(provider: string | null | undefined, tools: ToolPolicy): string | null {
  if (tools.subagents) {
    return 'subagents cannot be on: the contamination audit is not yet shown to see subagent work';
  }
  if (provider === 'anthropic' && tools.webSearch) {
    return 'webSearch cannot be on: Claude Code runs with --bare, which has no web search or fetch tools';
  }
  return null;
}

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
