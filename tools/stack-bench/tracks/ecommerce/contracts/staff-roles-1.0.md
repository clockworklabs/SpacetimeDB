# Staff role application interface

Use `staff-role-row` for each staff account and set `data-account-id` to that account's server
identifier. Put `staff-role-select` and `staff-role-save` inside the row.

The staff sign-in and staff-area controls come from the staff access feature.

Expose the same role assignment used by `staff-role-save`.

<!-- interface:http -->
Use `PUT /api/staff/:id/role`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `assign_staff_role` reducer.
<!-- /interface -->
