import type { NamedActionsCapability } from '../../actions/named-action-runtime.js';
import type { SpacetimeTarget } from '../stack-grading-operations.js';

type Raw = Record<string, unknown>;
type Schema = { reducers?: { name?: string; params?: { elements?: { algebraic_type?: Raw }[] } }[];
  typespace?: { types?: Raw[] } };

// SATS JSON for a parameter left blank, or undefined when the type has no blank value.
function emptyValue(schema: Schema, type: Raw | undefined, depth = 0): unknown {
  if (!type || depth > 8) return undefined;
  if (typeof type.Ref === 'number') return emptyValue(schema, schema.typespace?.types?.[type.Ref], depth + 1);
  if ('String' in type) return '';
  if ('Bool' in type) return false;
  if ('Array' in type) return [];
  const variants = (type.Sum as { variants?: { name?: { some?: string } }[] } | undefined)?.variants;
  if (variants?.length === 2 && variants[0]?.name?.some === 'some' && variants[1]?.name?.some === 'none') {
    return { none: [] };
  }
  return undefined;
}

// An interface that calls a reducer with no arguments leaves any extra
// parameters to the application ("Any extra checkout fields are optional").
// Send each one's blank value, as a form left empty would. A parameter with
// no blank value (a number, identity or record) keeps the call as given.
export async function withOptionalReducerArguments(target: SpacetimeTarget, reducer: string, args: string,
  fetchImpl: NamedActionsCapability['fetch'] = fetch): Promise<string> {
  if (args.replace(/\s/g, '') !== '[]') return args;
  const url = new URL(`/v1/database/${encodeURIComponent(target.mod)}/schema?version=9`, target.uri);
  let schema: Schema;
  try {
    const response = await fetchImpl(url.href, { method: 'GET', signal: AbortSignal.timeout(10_000) });
    if (!response.ok) return args;
    schema = JSON.parse(await response.text()) as Schema;
  } catch { return args; }
  const params = schema.reducers?.find(item => item.name === reducer)?.params?.elements ?? [];
  const values = params.map(param => emptyValue(schema, param.algebraic_type));
  return values.length && values.every(value => value !== undefined) ? JSON.stringify(values) : args;
}

// Reducer calls by HTTP name the reducer in the path.
export function spacetimeNamedActionFetch(fetchImpl: NamedActionsCapability['fetch'],
  target: SpacetimeTarget | null | undefined): NamedActionsCapability['fetch'] {
  if (!target) return fetchImpl;
  const prefix = new URL(`/v1/database/${encodeURIComponent(target.mod)}/call/`, target.uri).href;
  return async (url, options) => {
    const reducer = url.startsWith(prefix) ? decodeURIComponent(url.slice(prefix.length)) : '';
    if (!reducer || reducer.includes('/') || typeof options.body !== 'string') return fetchImpl(url, options);
    return fetchImpl(url, { ...options, body: await withOptionalReducerArguments(target, reducer, options.body, fetchImpl) });
  };
}
