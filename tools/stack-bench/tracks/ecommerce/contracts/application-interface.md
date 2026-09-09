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
