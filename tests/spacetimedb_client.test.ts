import { SpacetimeDBClient } from "../src/spacetimedb";
import WebsocketTestAdapter from "../src/websocket_test_adapter";
import Player from "./types/player";

describe("SpacetimeDBClient", () => {
  test("auto subscribe on connect", async () => {
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

    const players: Player[] = [];
    Player.onInsert((player: Player) => {
      players.push(player);
    });

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

    expect(players).toHaveLength(1);
    expect(players[0].ownerId).toBe("player-1");

    const transactionUpdate = {
      TransactionUpdate: {
        event: {
          timestamp: 1681391805281203,
          status: "committed",
          caller_identity: "identity-0",
          function_call: {
            reducer: "create_player",
            args: "[]",
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

    expect(players).toHaveLength(2);
    expect(players[1].ownerId).toBe("player-2");
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
            args: "[]",
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

    console.log(Player.all());
  });
});
