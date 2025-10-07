---
slug: /appendix
---

# Appendix

## SEQUENCE

For each table containing an `#[auto_inc]` column, SpacetimeDB creates a sequence number generator behind the scenes, which functions similarly to `postgres`'s `SEQUENCE`.

### How It Works

:::warning

Sequence number generation is not transactional.

:::

- Sequences in SpacetimeDB use Rust’s `i128` integer type.
- The field type marked with `#[auto_inc]` is cast to `i128` and increments by `1` for each new row.
- Sequences are pre-allocated in chunks of `4096` to speed up number generation, and then are only persisted to disk when the pre-allocated chunk is exhausted.
- Numbers are incremented even if a transaction is later rolled back.
- Unused numbers are not reclaimed, meaning sequences may have _gaps_.
- If the server restarts or a transaction rolls back, the sequence continues from the next pre-allocated chunk + `1`:

**Example:**

```rust
#[spacetimedb::table(name = users, public)]
struct Users {
    #[auto_inc]
    user_id: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn insert_user(ctx: &ReducerContext, count: u8) {
    for i in 0..count {
        let name = format!("User {}", i);
        ctx.db.users().insert(Users { user_id: 0, name });
    }
    // Query the table to see the effect of the `[auto_inc]` attribute:
    for user in ctx.db.users().iter() {
        log::info!("User: {:?}", user);
    }
}
```

Then:

```bash
❯ cargo run --bin spacetimedb-cli call sample insert_user 3

❯ spacetimedb-cli logs sample
...
.. User: Users { user_id: 1, name: "User 0" }
.. User: Users { user_id: 2, name: "User 1" }
.. User: Users { user_id: 3, name: "User 2" }

# Database restart, then

❯ cargo run --bin spacetimedb-cli call sample insert_user 1

❯ spacetimedb-cli logs sample
...
.. User: Users { user_id: 3, name: "User 2" }
.. User: Users { user_id: 4098, name: "User 0" }
```
