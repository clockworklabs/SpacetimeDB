# Notes for maintainers

The directory `src/client_api` is generated from [the SpacetimeDB client-api-messages](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/client-api-messages).
This is not automated.
Whenever the `client-api-messages` crate changes, you'll have to manually re-generate the definitions.
See that crate's DEVELOP.md for how to do this.

The generated files must be manually modified to fix their imports from the rest of the SDK.
Within each generated file:

- Change the import from `"@clockworklabs/spacetimedb-sdk"` to `"../index"`.
- If the type has generated a `class`, remove its `extends DatabaseTable`, remove the `public static db` member, and remove the call to `super()` within the constructor.
