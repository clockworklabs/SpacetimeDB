# Application interface

Use each interface name below as an exact HTML attribute value. Use `id` for
a one-off element. Use `data-role` when the same interface can
appear more than once. These attributes do not prescribe the layout, data
model, libraries, or transport.

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

Human-readable status text is case-insensitive. Machine identifiers and protocol values keep their specified spelling.
