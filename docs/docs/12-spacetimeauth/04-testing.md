---
title: Testing
slug: /spacetimeauth/testing
---

# Testing Your SpacetimeAuth Setup with OIDC Debugger

:::warning

SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.

:::

Before integrating SpacetimeAuth into your application code, it’s a good idea
to verify that your client and redirect URIs are working correctly. One of the
easiest ways to do this is with [OIDC Debugger](https://oidcdebugger.com).

## Why Use OIDC Debugger?

OIDC Debugger simulates the OAuth2 / OIDC Authorization Code flow in your browser.
It allows you to:

- Confirm that your **redirect URIs** are configured properly.
- Verify that your **client ID** works.
- Inspect the **ID Token** and claims (`email`, `sub`, `preferred_username`, etc.).
- Catch configuration issues before writing code.

---

## Step 1: Gather Your Configuration

- **Authorization Endpoint**:  
  `https://auth.spacetimedb.com/oidc/auth`

- **Token Endpoint**:  
  `https://auth.spacetimedb.com/oidc/token`

- **Client ID**: From your SpacetimeAuth dashboard, you can use any available client.
- **Redirect URI**: [https://oidcdebugger.com/debug](https://oidcdebugger.com/debug) must be added to your
  client’s allowed redirect URIs in the SpacetimeAuth dashboard.

---

## Step 2: Open OIDC Debugger

1. Go to [https://oidcdebugger.com](https://oidcdebugger.com).
2. Fill out the fields as follows, leave all other fields at their defaults
   (e.g., response type = code, state, nonce).

   | Field         | Value                                     |
   | ------------- | ----------------------------------------- |
   | Authorize URI | `https://auth.spacetimedb.com/oidc/auth`  |
   | Client ID     | Your SpacetimeAuth client ID              |
   | Scope         | `openid profile email` (or a subset)      |
   | Use PKCE?     | Checked                                   |
   | Token URI     | `https://auth.spacetimedb.com/oidc/token` |

:::warning
You do not need to enter the client secret here since the tool runs in the browser.
:::

![OIDC Debugger Setup](/images/spacetimeauth/oidcdebugger-config.png)

---

## Step 3: Run the Flow

1. Click **Send Request**.
2. Log in via SpacetimeAuth using any configured providers.
3. You’ll be redirected back to OIDC Debugger with an authorization code.
4. OIDC Debugger will automatically exchange the code for tokens and display the
   results.
   ![OIDC Debugger results](/images/spacetimeauth/oidcdebugger-results.png)

---

## Step 4: Inspect your tokens

Depending on the scopes you requested, you should receive an ID token like this:
![OIDC Debugger Result](/images/spacetimeauth/jwtio.png)

You can decode the ID token using any JWT decoder (e.g. [jwt.io](https://jwt.io/))
to see the claims included. For example:

```json
{
  "sub": "user_ergqg1q5eg15fdd54",
  "project_id": "project_xyz123",
  "email": "user@example.com",
  "email_verified": true,
  "preferred_username": "exampleuser",
  "first_name": "Example",
  "last_name": "User",
  "name": "Example User"
}
```
