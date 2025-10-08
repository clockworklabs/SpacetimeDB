---
title: React Integration
slug: /spacetimeauth/react-integration
---

# React Integration

:::warning

SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.

:::

This guide will walk you through integrating SpacetimeAuth into a React
application using the [react-oidc-context](https://www.npmjs.com/package/react-oidc-context)
library.
This library provides a simple way to handle OpenID Connect (OIDC) authentication
in React.

## Prerequisites

1. Create a SpacetimeAuth project and configure a client as described in the
   [Getting Started](/spacetimeauth/creating-a-project) and
   [Configuration](/spacetimeauth/configuring-a-project) guides.
2. Have a React application set up. You can use Create React App or any other
   React framework.
3. Install the `react-oidc-context` package in your React application:

## Configuring react-oidc-context

### 1. Add an OIDC configuration object

Create an OIDC configuration object with your SpacetimeAuth project details.
Make sure to replace `YOUR_CLIENT_ID` with the actual client ID from your
SpacetimeAuth dashboard.

```tsx
const oidcConfig = {
  authority: 'https://auth.spacetimedb.com/oidc',
  client_id: 'YOUR_CLIENT_ID',
  redirect_uri: `${window.location.origin}/callback`, // Where the user is redirected after login
  post_logout_redirect_uri: window.location.origin, // Where the user is redirected after logout
  scope: 'openid profile email',
  response_type: 'code',
  automaticSilentRenew: true,
};
```

### 2. Create a debug component

This component will log various authentication events and state changes to
the console for debugging purposes.

```tsx
export function OidcDebug() {
  const auth = useAuth();

  useEffect(() => {
    const ev = auth.events;

    const onUserLoaded = (u: any) =>
      console.log('[OIDC] userLoaded', u?.profile?.sub, u);
    const onUserUnloaded = () => console.log('[OIDC] userUnloaded');
    const onAccessTokenExpiring = () =>
      console.log('[OIDC] accessTokenExpiring');
    const onAccessTokenExpired = () => console.log('[OIDC] accessTokenExpired');
    const onSilentRenewError = (e: any) =>
      console.warn('[OIDC] silentRenewError', e);
    const onUserSignedOut = () => console.log('[OIDC] userSignedOut');

    ev.addUserLoaded(onUserLoaded);
    ev.addUserUnloaded(onUserUnloaded);
    ev.addAccessTokenExpiring(onAccessTokenExpiring);
    ev.addAccessTokenExpired(onAccessTokenExpired);
    ev.addSilentRenewError(onSilentRenewError);
    ev.addUserSignedOut(onUserSignedOut);

    return () => {
      ev.removeUserLoaded(onUserLoaded);
      ev.removeUserUnloaded(onUserUnloaded);
      ev.removeAccessTokenExpiring(onAccessTokenExpiring);
      ev.removeAccessTokenExpired(onAccessTokenExpired);
      ev.removeSilentRenewError(onSilentRenewError);
      ev.removeUserSignedOut(onUserSignedOut);
    };
  }, [auth.events]);

  useEffect(() => {
    console.log('[OIDC] state', {
      isLoading: auth.isLoading,
      isAuthenticated: auth.isAuthenticated,
      error: auth.error?.message,
      activeNavigator: auth.activeNavigator,
      user: !!auth.user,
    });
  }, [
    auth.isLoading,
    auth.isAuthenticated,
    auth.error,
    auth.activeNavigator,
    auth.user,
  ]);

  return null;
}
```

### 3. Wrap Your Application with AuthProvider

Wrap your React application with the `AuthProvider` component to provide
authentication context.

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { AuthProvider, useAuth } from 'react-oidc-context';
import App from './App';
import { OidcDebug } from './OidcDebug';

const oidcConfig = {...};

function onSigninCallback() {
  window.history.replaceState({}, document.title, window.location.pathname);
}

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
root.render(
  <AuthProvider {...oidcConfig} onSigninCallback={onSigninCallback}>
    <OidcDebug />
    <App />
  </AuthProvider>
);

```

### 4. Implement Authentication Logic in Your App

In your main component (e.g., `App.tsx`), use the `useAutoSignin` hook to automatically

sign in users if they are not authenticated.

```tsx
import React from 'react';

import { useAuth, useAutoSignin } from 'react-oidc-context';

import './App.css';

function App() {
  const auth = useAuth();

  useAutoSignin();

  if (auth.isLoading) {
    return <div>Loading...</div>;
  }

  if (auth.error) {
    return <div>Error: {auth.error.message}</div>;
  }

  if (!auth.isAuthenticated) {
    return <div>Redirecting to login...</div>;
  }

  return (
    <div className="App">
      <header className="App-header">
        Welcome, {auth.user?.profile.name} (id: {auth.user?.profile.sub})!
        <button onClick={() => auth.signoutRedirect()}>Sign Out</button>
      </header>
    </div>
  );
}
```

You're now set up to use SpacetimeAuth in your React application. When users
access your app, they will be redirected to the SpacetimeAuth login page for authentication.
