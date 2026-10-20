// Which of its own tools an agent may use beyond files and a shell: searching and reading
// the web, and starting other agents. Every agent gets the same policy, each adapter maps it
// to its own flags, and a campaign records it so its identity covers it.
export interface ToolPolicy {
  webSearch: boolean;
  subagents: boolean;
}

// Off by default, as every agent ran in StackBench 1.1; turning a tool on is its own condition.
export const DEFAULT_TOOL_POLICY: Readonly<ToolPolicy> = Object.freeze({ webSearch: false, subagents: false });

// Tools a provider's agent cannot have, and why. Claude Code runs with `--bare`, which offers
// only Bash, Edit and Read (checked on Claude Code 2.1.284 in the build image), so a policy that
// turns either on is refused rather than recorded for a run that could not honour it.
const UNAVAILABLE_TOOLS: Readonly<Record<string, { tools: ReadonlyArray<keyof ToolPolicy>; reason: string }>> = {
  anthropic: { tools: ['webSearch', 'subagents'],
    reason: 'Claude Code runs with --bare, which has no web search, web fetch or subagent tools' },
};

export function unavailableToolsReason(provider: string | null | undefined, tools: ToolPolicy): string | null {
  const unavailable = provider ? UNAVAILABLE_TOOLS[provider] : undefined;
  const requested = unavailable?.tools.filter(field => tools[field]) ?? [];
  return requested.length ? `${requested.join(' and ')} cannot be on: ${unavailable!.reason}` : null;
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
