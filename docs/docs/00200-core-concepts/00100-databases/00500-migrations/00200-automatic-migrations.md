---
title: Automatic Migrations
slug: /databases/automatic-migrations
---


When you republish a module to an existing database using `spacetime publish {database-name}`, SpacetimeDB attempts to automatically migrate your database schema to match the new module definition. This allows you to update your module code and redeploy without losing existing data, as long as the changes are compatible.

:::note
The "schema" refers to the collection of tables, reducers, procedures, views, and the types they depend on that are declared in your module code.
:::

## ✅ Safe Changes (Always Allowed)

The following changes are always allowed and will not break existing clients:

- **Adding new tables.** Non-updated clients will not be able to see them.
- **Adding indexes.**
- **Adding or removing `Auto Inc` annotations.**
- **Changing tables from private to public.**
- **Adding new reducers.**
- **Removing `Unique` constraints.**

## ⚠️ Potentially Breaking Changes

These changes are allowed by automatic migration, but may cause runtime errors for clients that haven't been updated:

- **Adding new columns to the end of a table with a default value.** The new column must be added at the end of the table definition and must have a default value specified. Non-updated clients will not be aware of the new column.
- **Changing or removing reducers.** Clients attempting to call the old version of a changed reducer or a removed reducer will receive runtime errors.
- **Changing tables from public to private.** Clients subscribed to a newly-private table will receive runtime errors.
- **Removing `Primary Key` annotations.** Non-updated clients will still use the old primary key as a unique key in their local cache, which can result in non-deterministic behavior when updates are received.
- **Removing indexes.** This is only breaking in specific situations. The main issue occurs with subscription queries involving semijoins, such as:

  ```sql
  SELECT Employee.*
  FROM Employee JOIN Dept
  ON Employee.DeptName = Dept.DeptName
  ```

  For performance reasons, SpacetimeDB will only allow this kind of subscription query if there are indexes on both join columns (`Employee.DeptName` and `Dept.DeptName`). Removing either index will invalidate this subscription query, resulting in client-side runtime errors.

## ❌ Forbidden Changes

The following changes cannot be performed with automatic migration and will cause the publish to fail:

- **Removing tables.**
- **Removing or modifying existing columns.** This includes changing the type, renaming, or reordering columns.
- **Adding columns without a default value.** New columns must have a default value so existing rows can be populated.
- **Adding columns in the middle of a table.** New columns must be added at the end of the table definition.
- **Changing whether a table is used for `scheduling`.**
- **Adding `Unique` or `Primary Key` constraints.** This could result in existing tables being in an invalid state.

## Working with Forbidden Changes

If you need to make changes that aren't supported by automatic migration, see [Incremental Migrations](/databases/incremental-migrations) for a production-ready pattern that allows complex schema changes without downtime or data loss.

For development and testing, you can use `spacetime publish --delete-data` to completely reset your database, but this should **not** be used in production as it permanently deletes all data.

## Best Practices

### During Development

- Use `--delete-data` freely during early development when data loss is acceptable
- Test migrations with sample data before applying to production databases
- Consider creating separate databases for development, staging, and production

### For Production

- **Plan schema changes carefully** - Review the migration compatibility rules before making changes
- **Coordinate with client updates** - When making potentially breaking changes, ensure clients are updated to handle the new schema
- **Use feature flags** - When adding new functionality, consider using feature flags in your reducers to enable gradual rollouts
- **Maintain backwards compatibility** - Where possible, add new tables/reducers rather than modifying existing ones
- **Document breaking changes** - Keep a changelog of schema changes that may affect clients

### Migration Strategies

For complex schema changes that aren't supported by automatic migration:

1. **Additive changes first** - Add new tables/columns before removing old ones
2. **Dual-write period** - Temporarily write to both old and new schema during transition
3. **Staged rollout** - Update clients to read from new schema while still supporting old schema
4. **Remove old schema** - Once all clients are updated, remove deprecated tables/columns

## Client Compatibility

During automatic migrations, active client connections are maintained and subscriptions continue to function. However:

- Clients may witness brief interruptions in scheduled reducers (such as game loops)
- New module versions may remove or change reducers, causing runtime errors for clients calling those reducers
- Clients won't automatically know about schema changes - you may need to regenerate and update client bindings

## Future Improvements

The SpacetimeDB team is working on enhanced migration capabilities, including:

- Support for more complex schema transformations
- Data migration scripts for table modifications
- Better tooling for previewing migration impacts
- Automatic client compatibility checking

Check the [SpacetimeDB documentation](https://spacetimedb.com/docs) and [GitHub repository](https://github.com/clockworklabs/SpacetimeDB) for the latest migration features and capabilities.
