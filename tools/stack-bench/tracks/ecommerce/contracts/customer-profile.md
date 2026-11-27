# Customer profile application interface

Use `catalog-link` to return to the catalog. If a profile overlay blocks navigation,
expose `overlay-close` to dismiss it. Saving may leave the profile open.

While signed in on the catalog, show `profile-link` directly or reveal it by clicking
`current-user`. No other navigation is required to reach it. Use it to open the profile.
Use `profile-name` and `profile-address` for the editable
values. Use `profile-save` to save them. Use `profile-address-summary` to display
the saved address in the profile view.

Expose `profile-save-state` as the status of the latest save, with `data-submit-state`:
`idle` initially, `pending` immediately when submitted, `succeeded` only after the
server confirms success, or `failed` after rejection or transport failure. Keep this
status visible if the editor closes. Keep its terminal state until the next submission
or page navigation. A new submission must replace the old state immediately.
