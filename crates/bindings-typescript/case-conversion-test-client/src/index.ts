import { DbConnection } from './module_bindings';
import type { Player2Status } from './module_bindings/player_2_status';

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

function assertEqual<T>(expected: T, found: T, message?: string): void {
  if (JSON.stringify(expected) !== JSON.stringify(found)) {
    throw new TestError(
      `${message || 'Assertion failed'}: Expected ${JSON.stringify(expected)} but found ${JSON.stringify(found)}`
    );
  }
}

async function connectThen<T>(callback: (db: DbConnection) => Promise<T>): Promise<T> {
  const name = dbNameOrPanic();
  
  return new Promise((resolve, reject) => {
    const conn = DbConnection.builder()
      .withDatabaseName(name)
      .withUri(LOCALHOST)
      .onConnect(async (ctx, identity, token) => {
        try {
          const result = await callback(ctx);
          ctx.disconnect();
          resolve(result);
        } catch (error) {
          ctx.disconnect();
          reject(error);
        }
      })
      .onConnectError((_ctx, error) => {
        reject(error);
      })
      .build();
  });
}

/// Test: Insert a player via createPlayer1 reducer using query builder subscription.
/// Verifies that table accessor `player1()`, field names with digits
/// (`player1Id`, `currentLevel2`, `status3Field`), and enum variant
/// `Player2Status::Active1` all work correctly through case conversion.
async function execInsertPlayer(): Promise<void> {
  await connectThen(async (db) => {
    return new Promise<void>((resolve, reject) => {
      let playerInserted = false;
      
      // Subscribe to player1 table changes
      db.db.player1.onInsert((_ctx, player) => {
        try {
          // Verify field names with digit boundaries are correctly case-converted
          assertEqual('Alice', player.playerName, 'Player name should match');
          assertEqual(5, player.currentLevel2, 'Current level should match');
          assertEqual('Active1', player.status3Field.tag, 'Status should be Active1');
          
          playerInserted = true;
        } catch (error) {
          reject(error);
        }
      });

      db.subscriptionBuilder()
        .onApplied(() => {
          // Create a player after subscription is applied
          db.reducers.createPlayer1({ Player1Name: 'Alice', Start2Level: 5 })
            .then(() => {
              // Give some time for the insert callback to fire
              setTimeout(() => {
                if (playerInserted) {
                  resolve();
                } else {
                  reject(new TestError('Player insert callback was not called'));
                }
              }, 100);
            })
            .catch(reject);
        })
        .onError((_ctx, error) => {
          reject(error);
        })
        .subscribe(q => q.from.player1().build());
    });
  });
}

/// Test: Insert a person via addPerson2 reducer using query builder subscription.
/// Verifies nested struct `Person3Info` with digit-boundary fields
/// (`ageValue1`, `scoreTotal`), index on `playerRef`, and
/// table accessor `person2()`.
async function execInsertPerson(): Promise<void> {
  await connectThen(async (db) => {
    return new Promise<void>((resolve, reject) => {
      let personInserted = false;
      let playerId: number;
      
      // Subscribe to person2 table changes
      db.db.person2.onInsert((_ctx, person) => {
        try {
          assertEqual('Bob', person.FirstName, 'Person first name should match');
          // Verify nested struct field names with digit boundaries
          assertEqual(25, person.personInfo.AgeValue1, 'Age should match');
          assertEqual(1000, person.personInfo.ScoreTotal, 'Score should match');
          assertEqual(playerId, person.playerRef, 'Player ref should match');
          
          personInserted = true;
        } catch (error) {
          reject(error);
        }
      });
      
      db.db.player1.onInsert((_ctx, player) => {
        playerId = player.Player1Id;
        // Now add a person referencing this player
        db.reducers.addPerson2({
          First3Name: 'Bob',
          playerRef: playerId,
          AgeValue: 25,
          ScoreTotal: 1000
        }).catch(reject);
      });

      db.subscriptionBuilder()
        .onApplied(() => {
          // Create a player first, then add a person referencing them
          db.reducers.createPlayer1({ Player1Name: 'PlayerForPerson', Start2Level: 1 })
            .then(() => {
              // Give some time for the callbacks to process
              setTimeout(() => {
                if (personInserted) {
                  resolve();
                } else {
                  reject(new TestError('Person insert callback was not called'));
                }
              }, 200);
            })
            .catch(reject);
        })
        .onError((_ctx, error) => {
          reject(error);
        })
        .subscribe([
          q => q.from.player1().build(),
          q => q.from.person2().build()
        ]);
    });
  });
}

/// Test: Ban a player via banPlayer1 reducer (which has explicit name `banPlayer1`).
/// Verifies that reducers with explicit names work, and that updating a player's
/// status from `Active1` to `BannedUntil(timestamp)` is reflected correctly.
async function execBanPlayer(): Promise<void> {
  await connectThen(async (db) => {
    return new Promise<void>((resolve, reject) => {
      let playerBanned = false;
      let playerId: number;
      
      // Subscribe to player1 table updates
      db.db.player1.onUpdate((_ctx, _old, updated) => {
        try {
          const status = updated.status3Field as Extract<Player2Status, { tag: 'BannedUntil' }>;
          assertEqual('BannedUntil', status.tag, 'Status should be BannedUntil');
          assertEqual(9999, status.value, 'Ban until value should match');
          
          playerBanned = true;
        } catch (error) {
          reject(error);
        }
      });
      
      db.db.player1.onInsert((_ctx, player) => {
        playerId = player.Player1Id;
        // Now ban this player using the explicit reducer name
        db.reducers.banPlayer1({
          Player1Id: playerId,
          BanUntil6: 9999
        }).catch(reject);
      });

      db.subscriptionBuilder()
        .onApplied(() => {
          // Insert a player to ban
          db.reducers.createPlayer1({ Player1Name: 'ToBan', Start2Level: 1 })
            .then(() => {
              // Give some time for the update callback to fire
              setTimeout(() => {
                if (playerBanned) {
                  resolve();
                } else {
                  reject(new TestError('Player update callback was not called'));
                }
              }, 200);
            })
            .catch(reject);
        })
        .onError((_ctx, error) => {
          reject(error);
        })
        .subscribe(q => q.from.player1().build());
    });
  });
}

/// Test: Query builder with a filter on a digit-boundary column.
/// Subscribes to player1 rows WHERE currentLevel2 == 5, verifying that
/// the case-converted column name works correctly in query builder filters.
async function execQueryBuilderFilter(): Promise<void> {
  await connectThen(async (db) => {
    return new Promise<void>((resolve, reject) => {
      let matchingPlayerSeen = false;
      
      // Subscribe with filter - only level-5 players should come through
      db.db.player1.onInsert((_ctx, player) => {
        try {
          // Only level-5 players should come through the filter
          assertEqual(5, player.currentLevel2, 'Player should be level 5');
          assertEqual('FilterMatch', player.playerName, 'Player name should be FilterMatch');
          
          matchingPlayerSeen = true;
        } catch (error) {
          reject(error);
        }
      });

      db.subscriptionBuilder()
        .onApplied(() => {
          // Insert a player at level 3 (should NOT match filter)
          db.reducers.createPlayer1({ Player1Name: 'NoMatch', Start2Level: 3 })
            .then(() => 
              // Insert a player at level 5 (should match filter)
              db.reducers.createPlayer1({ Player1Name: 'FilterMatch', Start2Level: 5 })
            )
            .then(() => {
              // Give some time for the insert callback to fire
              setTimeout(() => {
                if (matchingPlayerSeen) {
                  resolve();
                } else {
                  reject(new TestError('Matching player was not seen'));
                }
              }, 200);
            })
            .catch(reject);
        })
        .onError((_ctx, error) => {
          reject(error);
        })
        // Query builder: filter on digit-boundary column currentLevel2
        .subscribe(q => q.from.player1().where(p => p.currentLevel2.eq(5)).build());
    });
  });
}

/// Test: Query builder with a JOIN between player1 and person2.
/// Uses a right semijoin: person2 results from player1 JOIN person2.
/// This tests that:
/// - Digit-boundary column names work in join predicates
/// - The query builder correctly resolves canonical table names for both tables
/// - Joined results are received correctly through case-converted accessors
async function execQueryBuilderJoin(): Promise<void> {
  await connectThen(async (db) => {
    return new Promise<void>((resolve, reject) => {
      let joinPersonSeen = false;
      let playerId: number;
      
      // Listen for person2 inserts that come through the join.
      // The join is: player1 RIGHT SEMIJOIN person2 ON player1.Player1Id = person2.playerRef
      // This means we see person2 rows that have a matching player1 row.
      db.db.person2.onInsert((_ctx, person) => {
        // Only care about inserts from our join subscription
        if (person.FirstName === 'JoinPerson') {
          try {
            assertEqual('JoinPerson', person.FirstName, 'Person name should match');
            assertEqual(30, person.personInfo.AgeValue1, 'Age should match');
            assertEqual(500, person.personInfo.ScoreTotal, 'Score should match');
            assertEqual(playerId, person.playerRef, 'Player ref should match');
            
            joinPersonSeen = true;
          } catch (error) {
            reject(error);
          }
        }
      });
      
      db.db.player1.onInsert((_ctx, player) => {
        if (player.playerName === 'JoinedPlayer') {
          playerId = player.Player1Id;
          // Insert a person referencing this player — triggers the join
          db.reducers.addPerson2({
            First3Name: 'JoinPerson',
            playerRef: playerId,
            AgeValue: 30,
            ScoreTotal: 500
          }).catch(reject);
        }
      });

      db.subscriptionBuilder()
        .onApplied(() => {
          // Insert a player first
          db.reducers.createPlayer1({ Player1Name: 'JoinedPlayer', Start2Level: 7 })
            .then(() => {
              // Give some time for the join to process
              setTimeout(() => {
                if (joinPersonSeen) {
                  resolve();
                } else {
                  reject(new TestError('Join person was not seen'));
                }
              }, 300);
            })
            .catch(reject);
        })
        .onError((_ctx, error) => {
          reject(error);
        })
        .subscribe([
          // Query builder: JOIN player1 with person2 on Player1Id = playerRef
          // player1 RIGHT SEMIJOIN person2 means: show person2 rows that have a matching player1
          q => q.from.player1().rightSemijoin(q.from.person2(), (player, person) => 
            player.Player1Id.eq(person.playerRef)
          ).build(),
          // Also subscribe to player1 so reducer callbacks can see inserted players
          q => q.from.player1().build()
        ]);
    });
  });
}

async function main(): Promise<void> {
  const testName = process.argv[2];
  if (!testName) {
    throw new Error('Pass a test name as a command-line argument to the test client');
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
    console.log(`✅ Test "${testName}" passed`);
    process.exit(0);
  } catch (error) {
    console.error(`❌ Test "${testName}" failed:`, error);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});