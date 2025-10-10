using UnityEngine;
using SpacetimeDB;

public class SpacetimeDBClient : MonoBehaviour
{
    private const string HOST = "http://localhost:3000";
    private const string DB_NAME = "my-db";

    private DbConnection conn;

    void Start()
    {
        AuthToken.Init(".spacetime_token");

        conn = DbConnection.Builder()
            .WithUri(HOST)
            .WithModuleName(DB_NAME)
            .WithToken(AuthToken.Token)
            .OnConnect((token, identity, address) =>
            {
                Debug.Log($"Connected to SpacetimeDB");
                Debug.Log($"Identity: {identity}");

                // Subscribe to all tables
                conn.Subscribe(new System.Collections.Generic.List<string> { "SELECT * FROM *" });
            })
            .OnConnectError((error, message) =>
            {
                Debug.LogError($"Connection error: {error}");
            })
            .Build();
    }

    void OnDestroy()
    {
        conn?.Dispose();
    }
}
