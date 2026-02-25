/**
 * TypeScript SDK test client for case-conversion with Rust module `sdk-test-case-conversion`.
 *
 * Tests that:
 * - Table accessor names with digits (`player_1`, `person_2`) work correctly
 * - Field names with digit boundaries (`player1Id`, `currentLevel2`, `status3Field`) are converted
 * - Nested struct fields (`personInfo.ageValue1`) work
 * - Enum variants (`Player2Status`) work
 * - Reducers with explicit names (`banPlayer1`) work
 * - Query builder filters and joins work with case-converted names
 *
 * Driven by the Rust test harness via subcommands:
 *   node dist/index.js insert-player
 *   node dist/index.js insert-person
 *   node dist/index.js ban-player
 *   node dist/index.js query-builder-filter
 *   node dist/index.js query-builder-join
 */

import { DbConnection } from './module_bindings/index.js';

const LOCALHOST = 'http://localhost:3000';

function dbNameOrPanic(): string {
  const name = process.env.SPACETIME_SDK_TEST_DB_NAME;
  if (!name) {
    throw new Error('Failed to read db name from env');
  }
  return name;
}

class TestError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TestError';
  }
}

function assertEqual<T>(expected: T, found: T, label: string): void {
  const exp = JSON.stringify(expected);
  const fnd = JSON.stringify(found);
  if (exp !== fnd) {
    throw new TestError(`${label}: expected ${exp} but found ${fnd}`);
  }
}

/**
 * Connect to the database and run a callback once connected.
 * Returns a promise that resolves when the callback completes.
 */
function connectThen(callback: (db: DbConnection) => void): Promise<void> {
  const name = dbNameOrPanic();
  return new Promise<void>((resolve, reject) => {
    const conn = DbConnection.builder()
      .withDatabaseName(name)
      .withUri(LOCALHOST)
      .onConnect((ctx, _identity, _token) => {
        try {
          callback(ctx);
        } catch (error) {
          ctx.disconnect();
          reject(error);
        }
      })
      .onConnectError((_ctx, error) => {
        reject(error);
      })
      .build();

    // Store resolve/reject so tests can call them when done
    (conn as any).__resolve = resolve;
    (conn as any).__reject = reject;
  });
}

/**
 * Test: Insert a player via createPlayer1 reducer.
 * Verifies table accessor `player_1`, field names with digit boundaries
 * (`player1Id`, `currentLevel2`, `status3Field`), and enum variant `Active1`.
 */
async function execInsertPlayer(): Promise<void> {
  await connectThen((db) => {
    db.db.player_1.onInsert((_ctx, row) => {
      try {
        assertEqual('Alice', row.playerName, 'playerName');
        assertEqual(5, row.currentLevel2, 'currentLevel2');
        assertEqual('Active1', row.status3Field.tag, 'status3Field tag');
        db.disconnect();
        (db as any).__resolve();
      } catch (error) {
        db.disconnect();
        (db as any).__reject(error);
      }
    });

    db.subscriptionBuilder()
      .onError((_ctx) => {
        (db as any).__reject(new Error('Subscription errored'));
      })
      .onApplied((_ctx) => {
        db.reducers.createPlayer1({
          player1Name: 'Alice',
          start2Level: 5,
        });
      })
      .subscribe((q: any) => q.player_1().build());
  });
}

/**
 * Test: Insert a person via addPerson2 reducer.
 * Verifies nested struct `personInfo` with digit-boundary fields
 * (`ageValue1`, `scoreTotal`), and table accessor `person_2`.
 */
async function execInsertPerson(): Promise<void> {
  await connectThen((db) => {
    db.db.person_2.onInsert((_ctx, person) => {
      try {
        assertEqual('Bob', person.firstName, 'firstName');
        assertEqual(25, person.personInfo.ageValue1, 'ageValue1');
        assertEqual(1000, person.personInfo.scoreTotal, 'scoreTotal');
        db.disconnect();
        (db as any).__resolve();
      } catch (error) {
        db.disconnect();
        (db as any).__reject(error);
      }
    });

    db.db.player_1.onInsert((_ctx, player) => {
      db.reducers.addPerson2({
        first3Name: 'Bob',
        playerRef: player.player1Id,
        ageValue: 25,
        scoreTotal: 1000,
      });
    });

    db.subscriptionBuilder()
      .onError((_ctx) => {
        (db as any).__reject(new Error('Subscription errored'));
      })
      .onApplied((_ctx) => {
        db.reducers.createPlayer1({
          player1Name: 'PlayerForPerson',
          start2Level: 1,
        });
      })
      .subscribe((q: any) => [q.player_1().build(), q.person_2().build()]);
  });
}

/**
 * Test: Ban a player via banPlayer1 reducer (explicit name).
 * Verifies that reducers with explicit names work, and that updating a player's
 * status from `Active1` to `BannedUntil(9999)` is reflected correctly.
 */
async function execBanPlayer(): Promise<void> {
  await connectThen((db) => {
    db.db.player_1.onUpdate((_ctx, _old, updated) => {
      try {
        assertEqual('BannedUntil', updated.status3Field.tag, 'status tag');
        assertEqual(9999, (updated.status3Field as any).value, 'ban value');
        db.disconnect();
        (db as any).__resolve();
      } catch (error) {
        db.disconnect();
        (db as any).__reject(error);
      }
    });

    db.db.player_1.onInsert((_ctx, player) => {
      db.reducers.banPlayer1({
        player1Id: player.player1Id,
        banUntil6: 9999,
      });
    });

    db.subscriptionBuilder()
      .onError((_ctx) => {
        (db as any).__reject(new Error('Subscription errored'));
      })
      .onApplied((_ctx) => {
        db.reducers.createPlayer1({
          player1Name: 'ToBan',
          start2Level: 1,
        });
      })
      .subscribe((q: any) => q.player_1().build());
  });
}

/**
 * Test: Query builder with a filter on a digit-boundary column.
 * Subscribes to player_1 rows WHERE currentLevel2 == 5.
 */
async function execQueryBuilderFilter(): Promise<void> {
  await connectThen((db) => {
    db.db.player_1.onInsert((_ctx, row) => {
      try {
        assertEqual(5, row.currentLevel2, 'currentLevel2');
        assertEqual('FilterMatch', row.playerName, 'playerName');
        db.disconnect();
        (db as any).__resolve();
      } catch (error) {
        db.disconnect();
        (db as any).__reject(error);
      }
    });

    db.subscriptionBuilder()
      .onError((_ctx) => {
        (db as any).__reject(new Error('Subscription errored'));
      })
      .onApplied((_ctx) => {
        // Insert a player at level 3 (should NOT match filter)
        db.reducers.createPlayer1({
          player1Name: 'NoMatch',
          start2Level: 3,
        });
        // Insert a player at level 5 (should match filter)
        db.reducers.createPlayer1({
          player1Name: 'FilterMatch',
          start2Level: 5,
        });
      })
      .subscribe((q: any) =>
        q.player_1().where((p: any) => p.currentLevel2.eq(5)).build()
      );
  });
}

/**
 * Test: Query builder with a JOIN between player_1 and person_2.
 * Uses a right semijoin: player_1 RIGHT SEMIJOIN person_2 ON player1Id = playerRef.
 * Verifies digit-boundary column names work in join predicates and
 * joined results are received correctly.
 */
async function execQueryBuilderJoin(): Promise<void> {
  await connectThen((db) => {
    db.db.person_2.onInsert((_ctx, person) => {
      if (person.firstName === 'JoinPerson') {
        try {
          assertEqual('JoinPerson', person.firstName, 'firstName');
          assertEqual(30, person.personInfo.ageValue1, 'ageValue1');
          assertEqual(500, person.personInfo.scoreTotal, 'scoreTotal');
          db.disconnect();
          (db as any).__resolve();
        } catch (error) {
          db.disconnect();
          (db as any).__reject(error);
        }
      }
    });

    db.db.player_1.onInsert((_ctx, player) => {
      if (player.playerName === 'JoinedPlayer') {
        db.reducers.addPerson2({
          first3Name: 'JoinPerson',
          playerRef: player.player1Id,
          ageValue: 30,
          scoreTotal: 500,
        });
      }
    });

    db.subscriptionBuilder()
      .onError((_ctx) => {
        (db as any).__reject(new Error('Subscription errored'));
      })
      .onApplied((_ctx) => {
        db.reducers.createPlayer1({
          player1Name: 'JoinedPlayer',
          start2Level: 7,
        });
      })
      .subscribe((q: any) => [
        q.player_1()
          .rightSemijoin(q.person_2(), (player: any, person: any) =>
            player.player1Id.eq(person.playerRef)
          )
          .build(),
        q.player_1().build(),
      ]);
  });
}

async function main(): Promise<void> {
  const testName = process.argv[2];
  if (!testName) {
    throw new Error(
      'Pass a test name as a command-line argument to the test client'
    );
  }

  try {
    switch (testName) {
      case 'insert-player':
        await execInsertPlayer();
        break;
      case 'insert-person':
        await execInsertPerson();
        break;
      case 'ban-player':
        await execBanPlayer();
        break;
      case 'query-builder-filter':
        await execQueryBuilderFilter();
        break;
      case 'query-builder-join':
        await execQueryBuilderJoin();
        break;
      default:
        throw new Error(`Unknown test: ${testName}`);
    }
    console.log(`Test "${testName}" passed`);
    process.exit(0);
  } catch (error) {
    console.error(`Test "${testName}" failed:`, error);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});
