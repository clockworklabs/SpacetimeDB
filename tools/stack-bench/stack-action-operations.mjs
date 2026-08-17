import { leasedSpacetimeTarget } from './spacetime-target.mjs';

export function createSpacetimeGradingContext({ requireBuildContainer = true } = {}) {
  return leasedSpacetimeTarget({ requireBuildContainer });
}

export function createHttpGradingContext() {
  return null;
}

export function spacetimeNamedActionRequest({ action, input = {}, spacetime }) {
  const args = input.values === undefined
    ? (input.args ?? action.args ?? [])
    : (action.params ?? []).map(param => input.values[param.name]);
  return {
    url: spacetime && `${spacetime.uri}/v1/database/${spacetime.mod}/call/${action.reducer}`,
    body: JSON.stringify(args),
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
    body: JSON.stringify(body),
    missingNote: `no route at ${action.path}`,
  };
}
