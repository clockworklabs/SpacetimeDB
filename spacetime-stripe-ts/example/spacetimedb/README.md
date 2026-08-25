# Premium Store store module (stripe-ts example)

Per-app STDB module for `spacetime-stripe-example`. Owns the storefront's product catalog (`store_product`) and admin gating, and mounts the Stripe primitives from [`stripe-ts`](../../) under the `stripe` submodule.

This module exists to demonstrate how a real consumer integrates `stripe-ts`: the consumer brings their own STDB module for app-specific tables, mounts the submodule, and delegates to it through `ctx.as.stripe`.

## Tables

- `store_product`: public catalog rows
- `store_admin_identity`: private admin allowlist; fresh publishes seed the database owner from `init`

## Procedures

- `upsert_store_product`: admin-gated
- `seed_default_store_products({ force})`: admin-gated; idempotent unless `force`
- `set_store_product_price` / `clear_store_product_price`: admin-gated; links a Stripe price ID
- `list_store_products_json`: public read
- `add_admin_identity` / `remove_admin_identity`

## Publishing

```bash
spacetime publish --server http://127.0.0.1:3000 --yes spacetime-stripe-example
```

The parent test app's `pnpm run dev` calls this for you.

## License

[BSL 1.1](./LICENSE.txt), same as SpacetimeDB.
