# Staff role application interface

Put role management in the administrator area opened by `admin-link`.
If the role controls are on a separate tab or screen within it, expose `staff-roles-link`
there to open them. Omit this control when the role controls are already shown.
Use `staff-role-row` for each staff account and set `data-account-id` to that account's server
identifier. Put `staff-role-select` and `staff-role-save` inside the row.
On that row, expose `data-submit-state` for the latest role assignment: `idle`
initially, `pending` immediately when submitted, `succeeded` only after the server
confirms success, or `failed` after rejection or transport failure. Keep the terminal
state until the next submission. A new submission must replace the old state.
Also set the row's HTML `id` to `staff-role-account-` followed by
`encodeURIComponent(username)`, using the exact account username without changing its case.
For example, username `staff` has row ID `staff-role-account-staff`. This identifies the
account independently of the role options or other text in the row.

The staff sign-in and staff-area controls come from the staff access feature.

Also provide a second staff account named `staff2`, with the initial role `staff`.
Include it in role management like the other staff accounts.

Expose the same role assignment used by `staff-role-save`.

<!-- interface:http -->
Use `PUT /api/staff/:id/role`, where `:id` is the account identifier from `data-account-id`.
The JSON body is `{ "role": "<selected role>" }`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `assign_staff_role` reducer with arguments in this order: `accountId: u64`,
`role: string`. Render `data-account-id` as the decimal account identifier without precision loss.
<!-- /interface -->

<!-- interface:convex -->
Use `api:assign_staff_role` with `{ accountId, role }`. Render `data-account-id` as
the native string account identifier.
<!-- /interface -->

<!-- interface:supabase -->
Use the `assign_staff_role` PostgreSQL function with `{ accountId, role }`. Render
`data-account-id` as the account identifier accepted by `accountId`, without precision loss.
<!-- /interface -->

`staff-role-select` offers the roles `staff`, `inventory`, and `admin` as its option values.
