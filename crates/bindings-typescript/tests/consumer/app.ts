// A strict app on generated bindings that import the built package, typechecked
// by tests/consumer_types.test.ts. Generated schemas must keep satisfying the
// SDK's own constraints under exactOptionalPropertyTypes.
import { DbConnection } from '../../case-conversion-test-client/src/module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('test')
  .build();

export const created: Promise<void> = conn.reducers.createPlayer1({
  player1Name: 'player',
  start2Level: 1,
});
export const players = [...conn.db.player1.iter()];
