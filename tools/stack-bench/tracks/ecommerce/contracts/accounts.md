# Account application interface

Use these exact `id` attributes on the corresponding visible controls. They do not prescribe UI
structure, data modeling, libraries, or implementation strategy.

The signed-out page is the page the application's address opens for a signed-out visitor.
From the signed-out page, show the sign-up inputs, `signup-toggle`, or
`signin-toggle`. If sign-up is inside the sign-in dialog, opening `signin-toggle`
must reveal the sign-up inputs or `signup-toggle`. That control must reveal the
sign-up form. No other navigation is required to reach it.
Show the sign-in inputs or `signin-toggle` on the signed-out page.
While signed in, show `signout` directly or reveal it by clicking `current-user`.
No other navigation is required to reach sign-out.

| Element ID | Observable element |
|---|---|
| `signup-username` | sign-up username input |
| `signup-password` | sign-up password input |
| `signup-submit` | sign-up submit control |
| `signin-toggle` | control that reveals sign-in |
| `signup-toggle` | reveals sign-up; available on the signed-out page or after opening `signin-toggle`; omit when sign-up inputs are already visible |
| `signin-username` | sign-in username input |
| `signin-password` | sign-in password input |
| `signin-submit` | sign-in submit control |
| `current-user` | Active account name, present only while signed in. Do not use this hook for a signed-out message. |
| `signout` | sign-out control |
| `auth-error` | visible account error |

Accept any username of up to 48 characters made of letters, digits, and hyphens, and any
password of up to 64 characters.

Expose `window.getSessionToken()` as a synchronous function. For token-based
authentication, return the caller's existing token accepted by the application's
request transport, such as an HTTP header, native connection, or token argument.
For cookie-only authentication, return `null`. Do not return a cookie value unless
the application also accepts it through its token transport.
Return `null` when no such token exists. A database connection token can exist before
account login or after a rejected login. Return that token too; its presence does
not mean the caller has an authenticated application account.
This hook does not prescribe credential storage. Return the caller's real credential;
do not create a separate identity.

Account creation and login must use the application's real authentication path.
The interface does not prescribe password-taking backend function names or routes.
Keep credentials out of durable application logs. A failed registration or login
must not give the caller an application session or access to protected operations.
