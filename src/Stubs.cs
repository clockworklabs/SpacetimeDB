namespace SpacetimeDB
{
    public interface IReducerArgsBase : BSATN.IStructuralReadWrite
    {
        string ReducerName { get; }
    }

    public abstract class ReducerEventBase
    {
        public ulong Timestamp { get; }
        public SpacetimeDB.Identity? Identity { get; }
        public SpacetimeDB.Address? CallerAddress { get; }
        public string? ErrMessage { get; }
        public ClientApi.Event.Types.Status Status { get; }

        public ReducerEventBase()
        {
            Status = ClientApi.Event.Types.Status.Committed;
        }

        public ReducerEventBase(ClientApi.Event dbEvent)
        {
            Timestamp = dbEvent.Timestamp;
            Identity = SpacetimeDB.Identity.From(dbEvent.CallerIdentity.ToByteArray());
            CallerAddress = Address.From(dbEvent.CallerAddress.ToByteArray());
            ErrMessage = dbEvent.Message;
            Status = dbEvent.Status;
        }

        public abstract bool InvokeHandler();
    }
}
