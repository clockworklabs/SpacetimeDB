using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public interface IReducerArgsBase : BSATN.IStructuralReadWrite
    {
        string ReducerName { get; }
    }

    public abstract class ReducerEventBase
    {
        public ulong Timestamp { get; }
        public Identity? Identity { get; }
        public Address? CallerAddress { get; }
        public string? ErrMessage { get; }
        public UpdateStatus? Status { get; }

        public ReducerEventBase() { }

        public ReducerEventBase(TransactionUpdate update)
        {
            Timestamp = update.Timestamp.Microseconds;
            Identity = update.CallerIdentity;
            CallerAddress = update.CallerAddress;
            Status = update.Status;
            if (update.Status is UpdateStatus.Failed(var err))
            {
                ErrMessage = err;
            }
        }

        public abstract bool InvokeHandler();
    }
}
