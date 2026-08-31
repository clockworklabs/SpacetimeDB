import { leasedSpacetimeTarget } from '../runtime/spacetime-target.js';

interface NamedActionParameter {
  readonly name: string;
  readonly wireType?: string;
  readonly in?: string;
  readonly placeholder?: string;
}

interface NamedAction {
  readonly reducer?: string;
  readonly path: string;
  readonly method?: string;
  readonly args?: readonly unknown[];
  readonly params?: readonly NamedActionParameter[];
}

interface NamedActionInput {
  readonly args?: readonly unknown[];
  readonly body?: unknown;
  readonly values?: Readonly<Record<string, unknown>>;
}

interface SpacetimeTarget {
  readonly uri: string;
  readonly mod: string;
}

export function createSpacetimeGradingContext(
  { requireBuildContainer = true }: { requireBuildContainer?: boolean } = {},
) {
  return leasedSpacetimeTarget({ requireBuildContainer });
}

export function createHttpGradingContext() {
  return null;
}

const U64_MAX = 18_446_744_073_709_551_615n;

function spacetimeReducerBody(action: NamedAction, args: readonly unknown[]): string {
  const params = action.params ?? [];
  const encoded = args.map((value, index) => {
    if (params[index]?.wireType === 'u64') {
      const decimal = typeof value === 'string' ? value
        : (typeof value === 'number' && Number.isSafeInteger(value) ? String(value) : null);
      if (decimal === null || !/^(?:0|[1-9]\d*)$/.test(decimal)
          || BigInt(decimal) > U64_MAX) {
        const parameterName = params[index]?.name;
        const error = new Error(
          `invalid u64 value for reducer parameter ${JSON.stringify(parameterName)}`,
        ) as Error & { code: string };
        error.code = 'invalid_named_action_input';
        throw error;
      }
      return decimal;
    }
    return JSON.stringify(value) ?? 'null';
  });
  return `[${encoded.join(',')}]`;
}

export function spacetimeNamedActionRequest({
  action,
  input = {},
  spacetime,
}: {
  action: NamedAction;
  input?: NamedActionInput;
  spacetime?: SpacetimeTarget | null;
}) {
  const args = input.values === undefined
    ? (input.args ?? action.args ?? [])
    : (action.params ?? []).map(param => input.values?.[param.name]);
  return {
    url: spacetime && `${spacetime.uri}/v1/database/${spacetime.mod}/call/${action.reducer}`,
    method: 'POST',
    body: spacetimeReducerBody(action, args),
    missingNote: `no reducer named "${action.reducer}"`,
    // The reducer-call HTTP endpoint maps a reducer's deliberate application
    // failure to 530. This is not a generic server-error allowance: only this
    // adapter may classify that exact status as an application rejection.
    applicationRejectionStatuses: [530],
  };
}

export function httpNamedActionRequest({
  action,
  input = {},
  url,
}: {
  action: NamedAction;
  input?: NamedActionInput;
  url?: string | null;
}) {
  const base = String(url ?? '').replace(/\/$/, '');
  let path = action.path;
  let body = input.body ?? {};
  if (input.values !== undefined) {
    body = {};
    for (const param of action.params ?? []) {
      if (param.in === 'path') {
        if (!param.placeholder) {
          throw new TypeError(`path parameter ${JSON.stringify(param.name)} has no placeholder`);
        }
        path = path.replaceAll(
          param.placeholder,
          encodeURIComponent(String(input.values[param.name])),
        );
      } else {
        (body as Record<string, unknown>)[param.name] = input.values[param.name];
      }
    }
  }
  return {
    url: base ? `${base}${path}` : null,
    method: action.method ?? 'POST',
    body: JSON.stringify(body),
    missingNote: `no route at ${action.path}`,
  };
}
