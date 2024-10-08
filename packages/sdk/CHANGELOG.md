# Changelog

## 0.12.1

### Patch Changes

- [#107](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/107) [`2f6c82c`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/2f6c82c724b9f9407c7bedee13252ca8ffab8f7d) Thanks [@PuruVJ](https://github.com/PuruVJ)! - fix: websocket message handling, Buffer, onConnect

- [#108](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/108) [`b9db9b6`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/b9db9b6e46d8c98b29327d97c12c07b7a2fc96bf) Thanks [@PuruVJ](https://github.com/PuruVJ)! - docs: Public facing docs for 0.12

- [#105](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/105) [`79c278b`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/79c278be71b2dfd82106ada983fd81d395b1d912) Thanks [@PuruVJ](https://github.com/PuruVJ)! - fix: temporary token path invocation

## 0.12.0

### Minor Changes

- [#92](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/92) [`ab1f463`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/ab1f463d7da6e530a6cd47e2433141bfd16addd1) Thanks [@PuruVJ](https://github.com/PuruVJ)! - breaking: Flatten AlgebraicType & Simplify some codegen

- [#102](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/102) [`b8c944c`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/b8c944cd23d3b53c72131803a775127bf0a95213) Thanks [@cloutiertyler](https://github.com/cloutiertyler)! - internal: Remove global instance, allow multiple connections

### Patch Changes

- [#91](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/91) [`5adb557`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/5adb55776c81d0760cf0268df0fa5dee600f0ef8) Thanks [@PuruVJ](https://github.com/PuruVJ)! - types: Allow autocomplete in .on and .off types

- [#96](https://github.com/clockworklabs/spacetimedb-typescript-sdk/pull/96) [`17227c0`](https://github.com/clockworklabs/spacetimedb-typescript-sdk/commit/17227c0f65def3a9d5e767756ccf46777210041a) Thanks [@PuruVJ](https://github.com/PuruVJ)! - (fix) Synchronous WS Processing

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
