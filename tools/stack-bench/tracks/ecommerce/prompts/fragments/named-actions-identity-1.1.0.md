### Testing calls for accounts

Give the account writes the exact testing calls shown here so they can be made
without a browser. Use your stack's ordinary write path; do not add another
transport just for the benchmark.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| create an account | `POST /api/auth/signup` | reducer `signUp` |
| sign in | `POST /api/auth/signin` | reducer `signIn` |

The same uniqueness, credential, and authentication rules apply to these calls
as to the visible account interface.
