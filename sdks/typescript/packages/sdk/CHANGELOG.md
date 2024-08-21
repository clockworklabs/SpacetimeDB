# Changelog

## [0.8.0](https://github.com/clockworklabs/spacetimedb-typescript-sdk/compare/0.7.2...0.8.0) (2023-12-11)

### Bug Fixes

- Properly use BigInt for any numbers bigger than 32 bits
- Fix generating primary key names to be camel case

### Features

- Added ability to start multiple SpacetimeDB clients. New clients will have a separate ClientDB
- Changed the return type of functions returning table records - now they are arras instead of iterators
- Reducer callbacks have args passed in separately, which makes it easier to know what types they are
  For example a reducer taking a single string argument will have a callback signature like `(reducerEvent: ReducerEvent, name: string)`
  instead of `(reducerEvent: ReducerEvent, args: any[])`
- We now require explicitly registering any tables or reducers with `SpacetimeDBClient.registerReducers()` and `SpacetimeDBClient.registerTables()`.
  This also allows to register child classes, which in turn allows to use customized table classes. We will add more info
  on how to do it in the future. This makes it also harder to run into weird issues. If you only import a reducer, but not use
  it to set any callbacks, Node.js will filter out the import. If you then subscribe to a table SpacetimeDBClient will be unable
  to find the reducer. To ensure this is not happening people were adding a `console.log` statement listing and used classes to
  stop Node.js from filtering out any imports, like `console.log(SayHelloReducer)`. Now with the reducer call it's more explicit
- In this release we have also moved some methods from generated types into the SDK, which should result in a smaller footprint from
  generated classes
- Generated sum types are now easier to use. For sum types without any values you can use their type name as value, for example given an
  enum in Rust:

  ```rust
  enum UserRole {
      Admin,
      Moderator,
      User,
      Other(String)
  }
  ```

  you can now use types itself as values. For example given a reducer for setting a role you could now do the following in TypeScript:

  ```typescript
  SetRoleReducer.call(UserRole.Admin);
  SetRoleReducer.call(UserRole.Other('another role'));
  ```
