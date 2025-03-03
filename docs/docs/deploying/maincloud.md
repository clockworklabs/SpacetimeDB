# Deploy to Maincloud

Maincloud is a managed cloud service that provides developers an easy way to deploy their SpacetimeDB apps to the cloud.

## Deploy via CLI

1. Install the SpacetimeDB CLI for your platform: [Install SpacetimeDB](/install)
1. Create your module (see [Getting Started](/docs/getting-started))
1. Publish to Maincloud:

```bash
spacetime publish -s maincloud my-cool-module
```

## Connecting your Identity to the Web Dashboard

By logging in your CLI via spacetimedb.com, you can view your published modules on the web dashboard.

If you did not log in with spacetimedb.com when publishing your module, you can log in by running:
```bash
spacetime logout
spacetime login
```

1. Open the SpacetimeDB website and log in using your GitHub login.
1. You should now be able to see your published modules on the web dashboard.

---

With SpacetimeDB Cloud, you benefit from automatic scaling, robust security, and the convenience of not having to manage the hosting environment.
