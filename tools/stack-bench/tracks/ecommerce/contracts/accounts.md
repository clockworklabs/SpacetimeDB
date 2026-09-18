# Account application interface

Use these exact `id` attributes on the corresponding visible controls. They do not prescribe UI
structure, data modeling, libraries, or implementation strategy.

From the signed-out page, show the sign-up inputs, `signup-toggle`, or
`signin-toggle`. If sign-up is inside the sign-in dialog, opening `signin-toggle`
must reveal the sign-up inputs or `signup-toggle`. That control must reveal the
sign-up form. No other navigation is required to reach it.
Show the sign-in inputs or `signin-toggle` on the signed-out page.
While signed in, show `signout` directly or reveal it by clicking `current-user`.
No other navigation is required to reach sign-out.

When a supplied Keycloak service is available, you may use its hosted login
instead of local password forms. Put `data-auth-provider="keycloak"` on the
`signin-toggle` or `signup-toggle` control that opens it. The sign-in page must
also offer registration when there is no separate sign-up control. The provider's
visible form and errors replace the local form and error controls below. Keep
`current-user`, `signout`, and all business controls in the application.
Registration still requires only a username and password. Do not require email
or personal names merely to create an account. Preserve the requested display
name and existing username/password rules.

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

For bearer-token authentication, expose `window.getSessionToken()` as a synchronous
function that returns the current session's existing token, or `null` when signed out.
This hook does not prescribe credential storage. Return the caller's real credential;
do not create a separate identity.

Account creation and login must use the application's real authentication path.
The interface does not prescribe password-taking backend function names or routes.
Keep credentials out of durable application logs. A failed registration or login
must not give the caller an application session or access to protected operations,
even when authentication is handled by an external service.
