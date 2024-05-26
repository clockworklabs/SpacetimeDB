---
title: SpacetimeDB Cloud Deployment
navTitle: Testnet
---

The SpacetimeDB Cloud is a managed cloud service that provides developers an easy way to deploy their SpacetimeDB apps to the cloud.

Currently only the `testnet` is available for SpacetimeDB cloud which is subject to wipes. The `mainnet` will be available soon.

## Deploy via CLI

1. [Install](/install) the SpacetimeDB CLI.
1. Configure your CLI to use the SpacetimeDB Cloud. To do this, run the `spacetime server` command:

```bash
spacetime server add --default "https://testnet.spacetimedb.com" testnet
```

## Connecting your Identity to the Web Dashboard

By associating an email with your CLI identity, you can view your published modules on the web dashboard.

1. Get your identity using the `spacetime identity list` command. Copy it to your clipboard.
1. Connect your email address to your identity using the `spacetime identity set-email` command:

```bash
spacetime identity set-email <your-identity> <your-email>
```

1. Open the SpacetimeDB website and log in using your email address.
1. Choose your identity from the dropdown menu.
1. Validate your email address by clicking the link in the email you receive.
1. You should now be able to see your published modules on the web dashboard.

---

With SpacetimeDB Cloud, you benefit from automatic scaling, robust security, and the convenience of not having to manage the hosting environment.
