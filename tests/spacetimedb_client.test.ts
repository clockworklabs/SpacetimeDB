import { SpacetimeDBClient, ReducerEvent } from "../src/spacetimedb";
import WebsocketTestAdapter from "../src/websocket_test_adapter";
import Player from "./types/player";
import Point from "./types/point";
import CreatePlayerReducer from "./types/create_player_reducer";

describe("SpacetimeDBClient", () => {
  test("auto subscribe on connect", async () => {
    // so that TS doesn't remove the reducer import
    const _foo = CreatePlayerReducer;
    const client = new SpacetimeDBClient("ws://127.0.0.1:1234", "db");
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn(
      (
        _url: string,
        _headers: { [key: string]: string },
        _protocol: string
      ) => {
        return wsAdapter;
      }
    );

    client.subscribe("SELECT * FROM Player");
    client.subscribe(["SELECT * FROM Position", "SELECT * FROM Coin"]);

    client.connect();

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
    const client = new SpacetimeDBClient("ws://127.0.0.1:1234", "db");
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn(
      (
        _url: string,
        _headers: { [key: string]: string },
        _protocol: string
      ) => {
        return wsAdapter;
      }
    );

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    client.connect();

    wsAdapter.acceptConnection();
    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
        },
      },
    };
    wsAdapter.sendToClient(tokenMessage);

    expect(called).toBeTruthy();
  });

  test("it calls onInsert callback when a record is added with a subscription update and then with a transaction update", () => {
    const client = new SpacetimeDBClient("ws://127.0.0.1:1234", "db");
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn(
      (
        _url: string,
        _headers: { [key: string]: string },
        _protocol: string
      ) => {
        return wsAdapter;
      }
    );

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    client.connect();
    wsAdapter.acceptConnection();

    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
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
                row: ["player-1", "foo", [0, 0]],
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
          caller_identity: "identity-0",
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
                  row: ["player-2", "bar", [0, 0]],
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
    expect(inserts[1].reducerEvent?.callerIdentity).toBe("identity-0");
    expect(inserts[1].reducerEvent?.args).toEqual([
      "A Player",
      new Point(0.2, 0.3),
    ]);
  });

  test("it calls onUpdate callback when a record is added with a subscription update and then with a transaction update", () => {
    const client = new SpacetimeDBClient("ws://127.0.0.1:1234", "db");
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn(
      (
        _url: string,
        _headers: { [key: string]: string },
        _protocol: string
      ) => {
        return wsAdapter;
      }
    );

    let called = false;
    client.onConnect(() => {
      called = true;
    });

    client.connect();
    wsAdapter.acceptConnection();

    const tokenMessage = {
      data: {
        IdentityToken: {
          identity: "an-identity",
          token: "a-token",
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
          caller_identity: "identity-0",
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
                  row: ["player-2", "Jamie", [0, 0]],
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
    expect(updates[1]["oldPlayer"].name).toBe("Jamie");
    expect(updates[1]["newPlayer"].name).toBe("Kingslayer");
  });

  test("a reducer callback should be called after the database callbacks", () => {
    const client = new SpacetimeDBClient("ws://127.0.0.1:1234", "db");
    const wsAdapter = new WebsocketTestAdapter();
    client._setCreateWSFn(
      (
        _url: string,
        _headers: { [key: string]: string },
        _protocol: string
      ) => {
        return wsAdapter;
      }
    );

    client.connect();
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
          caller_identity: "identity-0",
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
});
