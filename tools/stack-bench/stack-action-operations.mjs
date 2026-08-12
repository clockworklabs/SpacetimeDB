import { leasedSpacetimeTarget } from './spacetime-target.mjs';

export function createSpacetimeGradingContext({ requireBuildContainer = true } = {}) {
  return leasedSpacetimeTarget({ requireBuildContainer });
}

export function createHttpGradingContext() {
  return null;
}

export function spacetimeNamedActionRequest({ action, input = {}, spacetime }) {
  return {
    url: spacetime && `${spacetime.uri}/v1/database/${spacetime.mod}/call/${action.reducer}`,
    body: JSON.stringify(input.args ?? action.args ?? []),
    missingNote: `no reducer named "${action.reducer}"`,
  };
}

export function httpNamedActionRequest({ action, input = {}, url }) {
  const base = String(url ?? '').replace(/\/$/, '');
  return {
    url: base ? `${base}${action.path}` : null,
    body: JSON.stringify(input.body ?? {}),
    missingNote: `no route at ${action.path}`,
  };
}
