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
    private readonly Identity _sender = sender;

    public Local Db { get; } = db;
    public ConnectionId? ConnectionId { get; } = connectionId;
    public Timestamp Timestamp { get; } = timestamp;
    public AuthCtx SenderAuth { get; } = senderAuth;
    public Random Rng { get; } = rng;

    public Identity Sender() => _sender;

    public TxContext WithTimestamp(Timestamp ts) =>
        new(Db, _sender, ConnectionId, ts, SenderAuth, Rng);
}
