namespace SpacetimeDB.Internal;

public sealed class TxContext(
    Local db,
    Identity sender,
    ConnectionId? connectionId,
    Timestamp timestamp,
    AuthCtx senderAuth,
    Random rng
)
{
    public Local Db { get; } = db;
    public Identity Sender { get; } = sender;
    public ConnectionId? ConnectionId { get; } = connectionId;
    public Timestamp Timestamp { get; } = timestamp;
    public AuthCtx SenderAuth { get; } = senderAuth;
    public Random Rng { get; } = rng;

    public TxContext WithTimestamp(Timestamp ts) =>
        new(Db, Sender, ConnectionId, ts, SenderAuth, Rng);
}
