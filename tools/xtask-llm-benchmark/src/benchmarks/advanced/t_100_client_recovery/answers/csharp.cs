using SpacetimeDB;
using SpacetimeDB.Types;

public sealed class ClientApp
{
    public DbConnection Connection { get; }
    public bool Ready { get; private set; }
    public string Outcome { get; private set; } = "idle";
    public int Changes { get; private set; }
    public int? Value => Connection.Db.Counter.Id.Find(0)?.Value;
    public ClientApp(string uri, string database)
    {
        Connection = DbConnection.Builder().WithUri(uri).WithDatabaseName(database)
            .WithAutomaticReconnect(new AutomaticReconnectOptions {
                MinDelay = TimeSpan.FromMilliseconds(500), MaxDelay = TimeSpan.FromSeconds(1)
            })
            .OnDisconnect((_, _) => Ready = false).Build();
        Connection.Db.Counter.OnUpdate += (_, _, _) => Changes++;
        Connection.Reducers.OnAdd += (ctx, _) => Outcome = ctx.Event.Status switch {
            Status.Committed => "committed", Status.UnknownResult => "unknown", _ => "failed"
        };
        Connection.SubscriptionBuilder().OnApplied(_ => Ready = true).Subscribe(new[] { "SELECT * FROM counter" });
    }
    public void Tick() => Connection.FrameTick();
    public void Add(int delta) { Outcome = "pending"; Connection.Reducers.Add(delta); }
    public void Stop() { Ready = false; Connection.Disconnect(); }
}
