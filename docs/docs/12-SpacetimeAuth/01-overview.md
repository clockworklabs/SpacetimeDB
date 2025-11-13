---
title: Overview
slug: /spacetimeauth
---

# SpacetimeAuth - Overview

:::warning

SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.

:::

SpacetimeAuth is a service for managing authentication for your SpacetimeDB
applications. This allows you to authenticate users without needing
an external authentication service or even a hosting server.
SpacetimeAuth is an [OpenID Connect (OIDC)](https://openid.net/developers/how-connect-works/) provider, which means it can be used with
any OIDC-compatible client library.

At the end of the authentication flow, your application receives an ID token
containing identity claims (such as email, username, and roles). Your
application can then use this token with any SpacetimeDB SDK to authenticate and
authorize users with the SpacetimeDB server.

## Features

### Authentication methods

- Magic link
- Github
- Google
- Discord
- Twitch
- Kick

### User & role management

- Create, update, and manage users
- Assign roles to users for role-based access control

### Customization

- Customizable theme for login pages
- Enable/disable anonymous and magic link authentication

## Terminology

Feel free to check out our blog post on OpenID
Connect [Who are you? Who am I to you?](https://spacetimedb.com/blog/who-are-you)
for more information about OpenID Connect and how it works.

### Projects

SpacetimeAuth uses the concept of "projects" to manage authentication for
different applications.

- Each project has its own set of users, roles, and authentication methods.
- Each project has its own configuration for email templates, web pages, and
  other settings.
- Each project is independent of a SpacetimeDB database and can be used by one
  or many databases.

Here are some examples of how you might use projects:

- You have a web application and a mobile application that both use the same
  SpacetimeDB database. You can create a single SpacetimeAuth project for both
  applications, therefore sharing a single user base.
- You have multiple SpacetimeDB databases for different environments (e.g. dev,
  staging, production). You can create a separate SpacetimeAuth project for each
  environment, therefore separating your users between environments.
- You have multiple SpacetimeDB databases for different applications. You can
  create a separate SpacetimeAuth project for each application, therefore
  separating your users between applications.

### Users

Users are the individuals who will be authenticating to your application via
SpacetimeAuth. Each user has a unique identifier (user ID) and can have one or
more roles assigned to them.

### Clients

:::note
Clients must not be confused with Users.
:::

Clients, also known as Relying Parties in OpenID Connect terminology, are the
applications that are relying on SpacetimeAuth for authentication. Each client
is associated with a single project and has its own client ID and client
secret.

Clients are applications that request an OpenID Connect ID token from
SpacetimeAuth, which can then be used to authenticate with the SpacetimeDB
server.

### Roles

Roles are used to manage access control within your application. Each role is
represented by a string (e.g. "admin", "user") that can be assigned to one or
more users.

Roles are included as claims in the ID token that is issued to the user upon
authentication.

Inside your reducers, you can check the user's roles to determine what
actions they are allowed to perform.

## Getting Started

Check out our [Creating a project guide](/spacetimeauth/creating-a-project) to
learn how to create and configure a SpacetimeAuth project.
