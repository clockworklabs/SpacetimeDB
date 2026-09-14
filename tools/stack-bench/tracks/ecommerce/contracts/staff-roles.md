# Staff role application interface

Put role management in the administrator area opened by `admin-link`.
Use `staff-role-row` for each staff account and set `data-account-id` to that account's server
identifier. Put `staff-role-select` and `staff-role-save` inside the row.
Also set the row's HTML `id` to `staff-role-account-` followed by
`encodeURIComponent(username)`, using the exact account username without changing its case.
For example, username `staff` has row ID `staff-role-account-staff`. This identifies the
account independently of the role options or other text in the row.

The staff sign-in and staff-area controls come from the staff access feature.

Expose the same role assignment used by `staff-role-save`.

<!-- interface:http -->
Use `PUT /api/staff/:id/role`, where `:id` is the account identifier from `data-account-id`.
The JSON body is `{ "role": "<selected role>" }`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `assign_staff_role` reducer with arguments in this order: `accountId: u64`,
`role: string`. Render `data-account-id` as the decimal account identifier without precision loss.
<!-- /interface -->

`staff-role-select` offers the roles `staff`, `inventory`, and `admin` as its option values.
