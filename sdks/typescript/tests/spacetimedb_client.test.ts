import { SpacetimeDBClient, ReducerEvent, ClientDB } from "../src/spacetimedb";
import { Identity } from "../src/identity";
import WebsocketTestAdapter from "../src/websocket_test_adapter";
import Player from "./types/player";
import User from "./types/user";
import Point from "./types/point";
import CreatePlayerReducer from "./types/create_player_reducer";
import { __SPACETIMEDB__ } from "../src/spacetimedb";

SpacetimeDBClient.registerTables(Player, User);
SpacetimeDBClient.registerReducers(CreatePlayerReducer);

beforeEach(() => {
  (CreatePlayerReducer as any).reducer = undefined;
  (Player as any).db = undefined;
  (User as any).db = undefined;
  __SPACETIMEDB__.clientDB = new ClientDB();
  __SPACETIMEDB__.spacetimeDBClient = undefined;
});

describe("SpacetimeDBClient", () => {
  test("auto subscribe on connect", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn((_url: string, _protocol: string) => {
      return wsAdapter;
    });

    client.subscribe("SELECT * FROM Player");
    client.subscribe(["SELECT * FROM Position", "SELECT * FROM Coin"]);

    await client.connect();

    wsAdapter.acceptConnection();

    const messages = wsAdapter.messageQueue;
    expect(messages.length).toBe(1);

    const message: object = JSON.parse(messages[0]);
    expect(message).toHaveProperty("subscribe");

    const expected = [
      "SELECT * FROM Player",
      "SELECT * FROM Position",
      "SELECT * FROM Coin",
    ];
    expect(message["subscribe"]["query_strings"]).toEqual(expected);
  });

  test("call onConnect callback after getting an identity", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn((_url: string, _protocol: string) => {
      return wsAdapter;
    });

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    await client.connect();

    wsAdapter.acceptConnection();
    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
          address: "00FF00",
        },
      },
    };
    wsAdapter.sendToClient(tokenMessage);

    expect(called).toBeTruthy();
  });

  test("it calls onInsert callback when a record is added with a subscription update and then with a transaction update", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn((_url: string, _protocol: string) => {
      return wsAdapter;
    });

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    await client.connect();
    wsAdapter.acceptConnection();

    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
          address: "00FF00",
        },
      },
    };
    wsAdapter.sendToClient(tokenMessage);

    type Insert = { player: Player; reducerEvent: ReducerEvent | undefined };
    let inserts: Insert[] = [];
    Player.onInsert(
      (player: Player, reducerEvent: ReducerEvent | undefined) => {
        inserts.push({ player, reducerEvent });
      }
    );

    let reducerCallbackLog: {
      reducerEvent: ReducerEvent;
      reducerArgs: any[];
    }[] = [];
    CreatePlayerReducer.on(
      (reducerEvent: ReducerEvent, name: string, location: Point) => {
        reducerCallbackLog.push({
          reducerEvent,
          reducerArgs: [name, location],
        });
      }
    );

    const subscriptionMessage = {
      SubscriptionUpdate: {
        table_updates: [
          {
            table_id: 35,
            table_name: "Player",
            table_row_operations: [
              {
                op: "insert",
                row_pk: "abcd123",
                row: ["player-1", "drogus", [0, 0]],
              },
            ],
          },
        ],
      },
    };
    wsAdapter.sendToClient({ data: subscriptionMessage });

    expect(inserts).toHaveLength(1);
    expect(inserts[0].player.ownerId).toBe("player-1");
    expect(inserts[0].reducerEvent).toBe(undefined);

    const transactionUpdate = {
      TransactionUpdate: {
        event: {
          timestamp: 1681391805281203,
          status: "committed",
          caller_identity: "00FF01",
          caller_address: "00FF00",
          function_call: {
            reducer: "create_player",
            args: '["A Player",[0.2, 0.3]]',
          },
          energy_quanta_used: 33841000,
          message: "a message",
        },
        subscription_update: {
          table_updates: [
            {
              table_id: 35,
              table_name: "Player",
              table_row_operations: [
                {
                  op: "insert",
                  row_pk: "abcdef",
                  row: ["player-2", "drogus", [0, 0]],
                },
              ],
            },
          ],
        },
      },
    };
    wsAdapter.sendToClient({ data: transactionUpdate });

    expect(inserts).toHaveLength(2);
    expect(inserts[1].player.ownerId).toBe("player-2");
    expect(inserts[1].reducerEvent?.reducerName).toBe("create_player");
    expect(inserts[1].reducerEvent?.status).toBe("committed");
    expect(inserts[1].reducerEvent?.message).toBe("a message");
    expect(inserts[1].reducerEvent?.callerIdentity).toEqual(
      Identity.fromString("00FF01")
    );
    expect(inserts[1].reducerEvent?.args).toEqual([
      "A Player",
      new Point(0.2, 0.3),
    ]);

    expect(reducerCallbackLog).toHaveLength(1);

    expect(reducerCallbackLog[0]["reducerEvent"]["callerIdentity"]).toEqual(
      Identity.fromString("00FF01")
    );
  });

  test("it calls onUpdate callback when a record is added with a subscription update and then with a transaction update", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn((_url: string, _protocol: string) => {
      return wsAdapter;
    });

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    await client.connect();
    wsAdapter.acceptConnection();

    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
          address: "00FF00",
        },
      },
    };
    wsAdapter.sendToClient(tokenMessage);

    const updates: { oldPlayer: Player; newPlayer: Player }[] = [];
    Player.onUpdate((oldPlayer: Player, newPlayer: Player) => {
      updates.push({
        oldPlayer,
        newPlayer,
      });
    });

    const subscriptionMessage = {
      SubscriptionUpdate: {
        table_updates: [
          {
            table_id: 35,
            table_name: "Player",
            table_row_operations: [
              {
                op: "delete",
                row_pk: "abcd123",
                row: ["player-1", "drogus", [0, 0]],
              },
              {
                op: "insert",
                row_pk: "def456",
                row: ["player-1", "mr.drogus", [0, 0]],
              },
            ],
          },
        ],
      },
    };
    wsAdapter.sendToClient({ data: subscriptionMessage });

    expect(updates).toHaveLength(1);
    expect(updates[0]["oldPlayer"].name).toBe("drogus");
    expect(updates[0]["newPlayer"].name).toBe("mr.drogus");

    const transactionUpdate = {
      TransactionUpdate: {
        event: {
          timestamp: 1681391805281203,
          status: "committed",
          caller_identity: "00FF01",
          caller_address: "00FF00",
          function_call: {
            reducer: "create_player",
            args: '["A Player",[0.2, 0.3]]',
          },
          energy_quanta_used: 33841000,
          message: "",
        },
        subscription_update: {
          table_updates: [
            {
              table_id: 35,
              table_name: "Player",
              table_row_operations: [
                {
                  op: "delete",
                  row_pk: "abcdef",
                  row: ["player-2", "Jaime", [0, 0]],
                },
                {
                  op: "insert",
                  row_pk: "123456",
                  row: ["player-2", "Kingslayer", [0, 0]],
                },
              ],
            },
          ],
        },
      },
    };
    wsAdapter.sendToClient({ data: transactionUpdate });

    expect(updates).toHaveLength(2);
    expect(updates[1]["oldPlayer"].name).toBe("Jaime");
    expect(updates[1]["newPlayer"].name).toBe("Kingslayer");
  });

  test("a reducer callback should be called after the database callbacks", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn((_url: string, _protocol: string) => {
      return wsAdapter;
    });

    await client.connect();
    wsAdapter.acceptConnection();

    let callbackLog: string[] = [];

    Player.onInsert(
      (player: Player, reducerEvent: ReducerEvent | undefined) => {
        callbackLog.push("Player");
      }
    );

    CreatePlayerReducer.on(() => {
      callbackLog.push("CreatePlayerReducer");
    });

    const transactionUpdate = {
      TransactionUpdate: {
        event: {
          timestamp: 1681391805281203,
          status: "committed",
          caller_identity: "00FF01",
          caller_address: "00FF00",
          function_call: {
            reducer: "create_player",
            args: '["A Player",[0.2, 0.3]]',
          },
          energy_quanta_used: 33841000,
          message: "a message",
        },
        subscription_update: {
          table_updates: [
            {
              table_id: 35,
              table_name: "Player",
              table_row_operations: [
                {
                  op: "insert",
                  row_pk: "abcdef",
                  row: ["player-2", "foo", [0, 0]],
                },
              ],
            },
          ],
        },
      },
    };
    wsAdapter.sendToClient({ data: transactionUpdate });

    expect(callbackLog).toEqual(["Player", "CreatePlayerReducer"]);
  });

  test("it calls onUpdate callback when a record is added with a subscription update and then with a transaction update when the PK is of type Identity", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn((_url: string, _protocol: string) => {
      return wsAdapter;
    });

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    await client.connect();
    wsAdapter.acceptConnection();

    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
          address: "00FF00",
        },
      },
    };
    wsAdapter.sendToClient(tokenMessage);

    const updates: { oldUser: User; newUser: User }[] = [];
    User.onUpdate((oldUser: User, newUser: User) => {
      updates.push({
        oldUser,
        newUser,
      });
    });

    const subscriptionMessage = {
      SubscriptionUpdate: {
        table_updates: [
          {
            table_id: 35,
            table_name: "User",
            table_row_operations: [
              {
                op: "delete",
                row_pk: "abcd123",
                row: [
                  "41db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008",
                  "drogus",
                ],
              },
              {
                op: "insert",
                row_pk: "def456",
                row: [
                  "41db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008",
                  "mr.drogus",
                ],
              },
            ],
          },
        ],
      },
    };
    wsAdapter.sendToClient({ data: subscriptionMessage });

    expect(updates).toHaveLength(1);
    expect(updates[0]["oldUser"].username).toBe("drogus");
    expect(updates[0]["newUser"].username).toBe("mr.drogus");

    const transactionUpdate = {
      TransactionUpdate: {
        event: {
          timestamp: 1681391805281203,
          status: "committed",
          caller_identity: "00FF01",
          caller_address: "00FF00",
          function_call: {
            reducer: "create_player",
            args: '["A User",[0.2, 0.3]]',
          },
          energy_quanta_used: 33841000,
          message: "",
        },
        subscription_update: {
          table_updates: [
            {
              table_id: 35,
              table_name: "User",
              table_row_operations: [
                {
                  op: "delete",
                  row_pk: "abcdef",
                  row: [
                    "11db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008",
                    "jaime",
                  ],
                },
                {
                  op: "insert",
                  row_pk: "123456",
                  row: [
                    "11db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008",
                    "kingslayer",
                  ],
                },
              ],
            },
          ],
        },
      },
    };
    wsAdapter.sendToClient({ data: transactionUpdate });

    expect(updates).toHaveLength(2);
    expect(updates[1]["oldUser"].username).toBe("jaime");
    expect(updates[1]["newUser"].username).toBe("kingslayer");
  });

  test("Filtering works", async () => {
    const client = new SpacetimeDBClient(
      "ws://127.0.0.1:1234",
      "db",
      undefined,
      "json"
    );
    const db = client.db;
    const user1 = new User(new Identity("bobs-idenitty"), "bob");
    const user2 = new User(new Identity("sallys-identity"), "sally");
    const users = db.getTable("User").instances;
    users.set("abc123", user1);
    users.set("def456", user2);

    const filteredUsers = User.with(client).filterByUsername("sally");
    expect(filteredUsers).toHaveLength(1);
    expect(filteredUsers[0].username).toBe("sally");
  });
});
