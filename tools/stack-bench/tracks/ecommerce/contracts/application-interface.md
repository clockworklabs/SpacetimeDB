# Application interface

Use each interface name below as an exact HTML attribute value. Use `id` for
a one-off element. Use `data-role` when the same interface can
appear more than once. These attributes do not prescribe the layout, data
model, libraries, or transport.

Each exposed server operation completes every valid request, including requests that
overlap, without relying on the browser to retry it. A business refusal, such as insufficient
stock, still completes the request.

<!-- interface:http -->
For error responses, use HTTP 401 or 403 for access errors and 400, 409, or 422 for input errors.
<!-- /interface -->

<!-- interface:reducer -->
Report reducer errors by failing the call.
<!-- /interface -->

<!-- interface:convex -->
Expose the named application operations below as public native Convex mutations in `convex/api.ts`.
Use the same mutations from the visible controls. Pass arguments as JSON objects with the
names shown below. Preserve native document identifiers as strings. Reject expected access
or input errors with `ConvexError`; successful calls use the normal Convex return value.
Do not add HTTP routes merely to wrap these mutations.
<!-- /interface -->

<!-- interface:supabase -->
Expose each named application operation below as a PostgreSQL function in the `public`
schema, called through the data API at `$SUPABASE_URL/rest/v1/rpc/<name>`. Use the same
functions from the visible controls. Name the parameters exactly as the JSON arguments
shown below. Raise expected access errors with SQLSTATE `42501`, and input errors or
business refusals with SQLSTATE `P0001` or `22023`; successful calls return the function's
normal result. Do not add Edge Functions or HTTP routes merely to wrap these functions.
<!-- /interface -->

Human-readable status text is case-insensitive. Machine identifiers and protocol values keep their specified spelling.
