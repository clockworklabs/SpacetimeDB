import { leasedSpacetimeTarget } from '../runtime/spacetime-target.mjs';

export function createSpacetimeGradingContext({ requireBuildContainer = true } = {}) {
  return leasedSpacetimeTarget({ requireBuildContainer });
}

export function createHttpGradingContext() {
  return null;
}

const U64_MAX = 18_446_744_073_709_551_615n;

function spacetimeReducerBody(action, args) {
  const params = action.params ?? [];
  const encoded = args.map((value, index) => {
    if (params[index]?.wireType === 'u64') {
      const decimal = typeof value === 'string' ? value
        : (typeof value === 'number' && Number.isSafeInteger(value) ? String(value) : null);
      if (decimal === null || !/^(?:0|[1-9]\d*)$/.test(decimal)
          || BigInt(decimal) > U64_MAX) {
        const error = new Error(`invalid u64 value for reducer parameter ${JSON.stringify(params[index].name)}`);
        error.code = 'invalid_named_action_input';
        throw error;
      }
      return decimal;
    }
    return JSON.stringify(value) ?? 'null';
  });
  return `[${encoded.join(',')}]`;
}

export function spacetimeNamedActionRequest({ action, input = {}, spacetime }) {
  const args = input.values === undefined
    ? (input.args ?? action.args ?? [])
    : (action.params ?? []).map(param => input.values[param.name]);
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

export function httpNamedActionRequest({ action, input = {}, url }) {
  const base = String(url ?? '').replace(/\/$/, '');
  let path = action.path;
  let body = input.body ?? {};
  if (input.values !== undefined) {
    body = {};
    for (const param of action.params ?? []) {
      if (param.in === 'path') {
        path = path.replaceAll(param.placeholder, encodeURIComponent(String(input.values[param.name])));
      } else {
        body[param.name] = input.values[param.name];
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
