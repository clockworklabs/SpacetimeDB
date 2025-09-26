# SpacetimeAuth - Overview

> ⚠️ SpacetimeAuth is currently in beta

SpacetimeAuth is a service for managing authentication for your SpacetimeDB
applications. This allows you to authenticate users without needing
an external authentication service or even a hosting server.
SpacetimeAuth is an OpenID Connect (OIDC) provider, which means it can be used with
any OIDC-compatible client library.

## Features

- User management: create, update, delete users
- Multiple authentication methods:
  - Magic link
  - Github
  - Google
  - Discord
  - Twitch
  - Kick
  - More providers will be added in the future
- Role-based access control: assign roles to users
- Customizable templates for emails and web pages
- (Comming soon) Steam integration: authenticate users via Steam and ensure they
  own a specific game or DLC

## Terminology

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
  applications.
- You have multiple SpacetimeDB databases for different environments (e.g. dev,
  staging, production). You can create a separate SpacetimeAuth project for each
  environment.
- You have multiple SpacetimeDB databases for different applications. You can
  create a separate SpacetimeAuth project for each application.

### Users

Users are the individuals who will be authenticating to your application via
SpacetimeAuth. Each user has a unique identifier (user ID) and can have one or
more roles assigned to them.

### Clients

> ⚠️ Clients must not be confused with Users.

Clients (from OpenID Connect) are the applications that will be using
SpacetimeAuth for authentication. Each client is associated with a single
project and has its own client ID and client secret.

### Roles

Roles are used to manage access control within your application. Each role is
just a string (e.g. "admin", "user") that can be assigned to one or more users.
Roles are included as claims in the ID token that is issued to the user upon
authentication.

Inside your reducers, you can check the user's roles to determine what
actions they are allowed to perform.

## Getting Started

Check out the [Setup guide](setup.md) to learn how to create and
configure a SpacetimeAuth project.
