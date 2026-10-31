// Whether this environment lets us compile code from strings with `Function`.
//
// Generating (de)serializers with `Function` is faster, but some environments
// forbid it: browsers with a Content-Security-Policy that lacks
// `'unsafe-eval'`, Cloudflare Workers, and Node with
// `--disallow-code-generation-from-strings`. In those we fall back to closures.
//
// We probe once and cache the answer, because under a CSP every failed
// attempt also fires a `securitypolicyviolation` report.
let allowed: boolean | undefined;

export function canCompileFunctions(): boolean {
  if (allowed === undefined) {
    try {
      Function('');
      allowed = true;
    } catch {
      allowed = false;
    }
  }
  return allowed;
}

/**
 * Overrides the result of {@link canCompileFunctions}. Pass `undefined` to
 * probe the environment again. For tests only.
 *
 * (De)serializers are cached per type, so this only affects types whose
 * (de)serializers have not been built yet.
 */
export function setCanCompileFunctionsForTesting(value: boolean | undefined) {
  allowed = value;
}
