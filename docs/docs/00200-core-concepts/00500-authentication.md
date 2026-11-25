# Authentication

SpacetimeDB modules are exposed to the open internet and anyone can connect to
them. Therefore, authentication is a critical part of using SpacetimeDB securely.

SpacetimeDB uses OpenID Connect (OIDC) identity tokens for authentication, making
it compatible with most OIDC providers (e.g., Auth0, Firebase, Clerk, Google,
GitHub, Facebook, and many more). You can choose any OIDC provider that fits your
needs, or even [run your own](#running-your-own-provider).

If you're new to OIDC, check out our [blog post about OIDC](https://spacetimedb.com/blog/who-are-you)
to learn more about how OIDC works and why it's a great choice for authentication.

## How SpacetimeDB verifies a token

![How SpacetimeDB verifies an OIDC token](/images/authentication/oidc-flow.svg)

1. The user signs in with your OIDC provider, which may be SpacetimeAuth, a
   third-party service, or a server you run yourself.
2. Optionally, the provider sends the user through another login, such as Google.
   SpacetimeDB never sees this step.
3. The provider signs a JWT and returns it to your client. The token names the
   provider that issued it (`iss`), the user (`sub`), and the application it was
   issued for (`aud`).
4. Your client connects to SpacetimeDB with the token.
5. SpacetimeDB fetches `{iss}/.well-known/openid-configuration` from the URL in
   the token's `iss` claim, then fetches the provider's public keys from the
   `jwks_uri` listed there. It caches the keys and refreshes them every few
   minutes.
6. SpacetimeDB checks the token's signature and expiry, computes the user's
   `Identity` from `iss` and `sub`, and runs your module's `client_connected`
   reducer, where you decide whether to accept the connection.

SpacetimeDB accepts a token from any provider that follows these steps, and it
does not check `aud`, so your module should check `iss` and `aud` itself, as
shown in [Using Auth Claims](./00500-authentication/00500-usage.md#example-restricting-auth-providers).

### Running your own provider

To issue tokens from your own server, serve the discovery document and a
[JSON Web Key Set](https://datatracker.ietf.org/doc/html/rfc7517) at publicly
reachable URLs, as in step 5, and sign tokens that meet these rules:

- Sign with `ES256` or `RS256`. SpacetimeDB rejects tokens signed with a shared
  secret, such as `HS256`.
- Include `iss`, `sub` and `iat`. `iss` and `sub` must be non-empty and at most
  128 bytes, and `iss` must be the URL your discovery document is served under.
- Include a short `exp`. SpacetimeDB checks expiry only when a token is presented,
  so a connection that is already open stays open after its token expires.
- Keep `sub` stable for each user, and don't change your issuer URL, because the
  user's `Identity` is computed from the exact `iss` and `sub` strings.
- When you rotate signing keys, publish the new public key well before you start
  signing with it, because SpacetimeDB may still be using its cached keys.

## Server-issued tokens and reconnects

If a client connects without supplying a token, SpacetimeDB creates a new
identity and returns a server-issued token for that identity. If you want later
connections to keep using that same identity, persist that returned token and
pass it back into the SDK on reconnect.

On platforms that can send an `Authorization` header during the WebSocket
handshake, the SDK can reuse that saved token directly. On platforms that cannot
set custom WebSocket headers, the SDK first exchanges the saved token for a
short-lived WebSocket token through
[`POST /v1/identity/websocket-token`](/docs/http/identity#post-v1identitywebsocket-token),
then sends only that short-lived token in the WebSocket URL.

This matters when you persist tokens on the client:

- Save the long-lived server-issued token that establishes the user's identity.
- Do not overwrite an already-saved long-lived token with a short-lived
  WebSocket token received during reconnect.
- Expect this distinction on browser-style transports where WebSocket headers
  are unavailable, such as Unity WebGL builds.

For C# clients with `.WithAutomaticReconnect()` enabled, the SDK retains the
authentication token and reuses it during recovery. `OnConnect` receives it on
the initial connection; `OnAutomaticReconnect` receives the retained or refreshed
token after each successful recovery. Persist tokens from both callbacks for
future application sessions. This also applies on Unity WebGL: these callbacks
receive the authentication token, rather than the transport's short-lived token.
The WebGL precaution above still applies to C# connections without automatic reconnect.

For expiring credentials, also configure `.WithTokenProvider(() => RefreshTokenAsync())`.
Supply the initial token through `.WithToken(initialToken)`; the provider is only
used during automatic reconnect. It returns a token for the same identity when
the retained token is near expiry, its expiry cannot be read, or a reused token
has been rejected. The SDK does not refresh tokens periodically while connected.
See the [C# token-provider reference](./00600-clients/00600-csharp-reference.md#method-withtokenprovider)
for refresh timing, rejection handling, and cancellation behavior.

## SpacetimeAuth

To make it easier to get started with authentication, SpacetimeDB offers
[SpacetimeAuth](./00500-authentication/00100-spacetimeauth/index.md), a fully
managed OIDC provider built specifically for SpacetimeDB applications. SpacetimeAuth
handles user management, authentication flows, and token issuance, so you don't have
to worry about building and maintaining your own authentication service.

SpacetimeAuth is meant to be simple to use and easy to integrate with SpacetimeDB.
While being production-ready and able to support most common use cases, it is not
as feature-rich as some third-party OIDC providers. If you need advanced features
or customization, you may want to consider using a third-party OIDC provider instead.

## Third-party OIDC providers

You can also use any third-party OIDC provider with SpacetimeDB. Most OIDC
providers offer similar features, such as user management, authentication flows,
and token issuance. When choosing a third-party OIDC provider, consider factors
such as ease of integration, pricing, scalability, and security.

- [Auth0](https://auth0.com/) A managed identity and access
  management service that provides, user management, and extensible login flows
  for applications and APIs.
- [Clerk](https://clerk.com/) A developer-focused
  authentication and user management platform that provides OIDC-compliant
  sign-in, session management, and prebuilt UI components for modern web applications.
- [Keycloak](https://www.keycloak.org/) An open-source and
  self-hosted OIDC provider with extensive features, customization options and integrations.

## Authenticate your services

Sometimes, you may need to authenticate your servers, APIs or other services that
interact with your SpacetimeDB database. OIDC tokens can also be used for this
purpose, allowing secure communication between your services and SpacetimeDB.

To authenticate your services, you have a few options depending on your OIDC provider:

- **Client credentials flow**: Many OIDC providers support the client credentials
  flow, which allows your service to obtain an access token using its own
  credentials (client ID and client secret). This is a common approach for
  service-to-service authentication.
- **Service accounts**: Some OIDC providers offer service accounts, which are
  special user accounts designed for non-human users (e.g., servers, APIs). You
  can create a service account and use its credentials to obtain an access token.

## Authorization in your module

Obtaining an OIDC token is just the first step in securing your SpacetimeDB
module, known as **authentication**. You also need to implement **authorization**
to control what authenticated users can do within your module.

When a client connects to your SpacetimeDB module, the SpacetimeDB server
validates the client's OIDC token and extracts the identity claims. These claims
are then made available to your module's reducers, views and procedures via the context.

[Check out the usage guide](./00500-authentication/00500-usage.md) for more
information on how to access and use authentication claims in your module:
