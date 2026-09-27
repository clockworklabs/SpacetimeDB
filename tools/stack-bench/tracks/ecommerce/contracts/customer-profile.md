# Customer profile application interface

Use `catalog-link` to return to the catalog. If a profile overlay blocks navigation,
expose `overlay-close` to dismiss it. Saving may leave the profile open.

While signed in on the catalog, show `profile-link` directly or reveal it by clicking
`current-user`. No other navigation is required to reach it. Use it to open the profile.
Use `profile-name` and `profile-address` for the editable
values. Use `profile-save` to save them. Use `profile-address-summary` to display
the saved address in the profile view.
