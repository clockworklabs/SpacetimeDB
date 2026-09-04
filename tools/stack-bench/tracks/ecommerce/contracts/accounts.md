# Account application interface

Use these exact `id` attributes on the corresponding visible controls. They do not prescribe UI
structure, data modeling, libraries, or implementation strategy.

| Element ID | Observable element |
|---|---|
| `signup-username` | sign-up username input |
| `signup-password` | sign-up password input |
| `signup-submit` | sign-up submit control |
| `signin-toggle` | control that reveals sign-in |
| `signin-username` | sign-in username input |
| `signin-password` | sign-in password input |
| `signin-submit` | sign-in submit control |
| `current-user` | active account name |
| `signout` | sign-out control |
| `auth-error` | visible account error |

Expose the same account writes used by the UI.

<!-- interface:http -->
Use `POST /api/auth/signup` and `POST /api/auth/signin`. Both accept JSON with
`username` and `password` fields.
<!-- /interface -->

<!-- interface:reducer -->
Use the `signUp` and `signIn` reducers. Both take `username` and `password`, in that order.
<!-- /interface -->
