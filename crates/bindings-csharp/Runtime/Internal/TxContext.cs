namespace SpacetimeDB.Internal;

public sealed class TxContext
{
    public TxContext(
        Local db,
        Identity sender,
        ConnectionId? connectionId,
        Timestamp timestamp,
        AuthCtx senderAuth,
        Random rng)
    {
        Db = db;
        Sender = sender;
        ConnectionId = connectionId;
        Timestamp = timestamp;
        SenderAuth = senderAuth;
        Rng = rng;
    }

    public Local Db { get; }
    public Identity Sender { get; }
    public ConnectionId? ConnectionId { get; }
    public Timestamp Timestamp { get; }
    public AuthCtx SenderAuth { get; }
    public Random Rng { get; }

    public TxContext WithTimestamp(Timestamp ts) =>
        new(Db, Sender, ConnectionId, ts, SenderAuth, Rng);
}