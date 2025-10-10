using SpacetimeDB;

const string HOST = "http://localhost:3000";
const string DB_NAME = "my-db";

AuthToken.Init(".spacetime_token");

var conn = DbConnection.Builder()
    .WithUri(HOST)
    .WithModuleName(DB_NAME)
    .WithToken(AuthToken.Token)
    .OnConnect((token, identity, address) =>
    {
        Console.WriteLine($"Connected to SpacetimeDB");
        Console.WriteLine($"Identity: {identity}");

        // Subscribe to all tables
        conn.Subscribe(new List<string> { "SELECT * FROM *" });
    })
    .OnConnectError((error, message) =>
    {
        Console.WriteLine($"Connection error: {error}");
        Environment.Exit(1);
    })
    .Build();

// Run the connection
Console.WriteLine("Press Ctrl+C to exit");
while (true)
{
    Thread.Sleep(100);
}
