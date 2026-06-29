---
title: Railway
slug: /how-to/deploy/railway
---

Railway is a hosted platform for deploying infrastructure and application services. If you want to run SpacetimeDB without managing your own VM, the official Railway template is a quick way to get started.

The template deploys the first-party `clockworklabs/spacetime` image, exposes port `3000`, and provisions persistent storage at `/stdb`. Once the service is running, you can publish one or more databases to it with the SpacetimeDB CLI.

## Prerequisites

1. A [Railway account](https://railway.com/)
2. The SpacetimeDB CLI installed: [Install SpacetimeDB](https://spacetimedb.com/install)
3. A SpacetimeDB module project ready to publish

## Step 1: Deploy the Railway template

Open the official deployment template:

[SpacetimeDB Template](https://railway.com/deploy/spacetimedb)

Then:

1. Click **Deploy Now**.
2. Create a new Railway project or choose an existing one.
3. Wait for the deployment to finish.
4. In Railway, open your service and copy its public domain or attach a custom domain.

That domain is the base URL your CLI and clients will use to connect to this SpacetimeDB instance.

## Step 2: Add the Railway deployment to your CLI

Register your Railway deployment as a named server:

```bash
spacetime server add --url https://<your-railway-domain> railway
```

For example:

```bash
spacetime server add --url https://my-railway-app.up.railway.app railway
```

You can optionally verify the connection:

```bash
spacetime server ping railway
```

## Step 3: Publish your database

From your SpacetimeDB project, publish a database to the Railway deployment:

```bash
spacetime publish my-database --server railway
```

To update an existing database later, run the same command again.

## Step 4: Connect clients

After publishing, connect your client to your Railway-hosted database using your Railway domain as the server URI and your database name.

See [Connecting to SpacetimeDB](../../../00200-core-concepts/00600-clients/00300-connection.md) for the current client connection patterns across supported SDKs.

## Notes

- The Railway template sets up the SpacetimeDB server itself, but it does not publish your module for you. You still deploy your database schema and logic with `spacetime publish`.
- A single Railway-hosted SpacetimeDB instance can host multiple databases.
- If you want full control over the host, reverse proxy, and operating system setup, see [Self-hosting](./00200-self-hosting.md).
