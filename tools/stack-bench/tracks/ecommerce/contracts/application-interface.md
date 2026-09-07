# Application interface

Use each interface name below as an exact HTML attribute value. Use `id` for
a one-off element. Use `data-role` when the same interface can
appear more than once. These attributes do not prescribe the layout, data
model, libraries, or transport.

<!-- interface:http -->
Answer a request that the signed-in account or a signed-out visitor is not allowed to make
with HTTP 401 or 403. Answer a request whose values are invalid with HTTP 400, 409, or 422.
<!-- /interface -->

<!-- interface:reducer -->
A reducer refuses a request the caller is not allowed to make, or whose values are invalid,
by failing.
<!-- /interface -->
