# Account testing interface

The runner locates visible controls through exact `data-testid` attributes. These attributes do
not prescribe UI structure, data modeling, libraries, or implementation strategy.

| Test ID | Observable element |
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

The runner also issues the same account writes as the UI. Server-based stacks expose
`POST /api/auth/signup` and `POST /api/auth/signin`. SpacetimeDB modules expose `signUp` and
`signIn` reducers.
