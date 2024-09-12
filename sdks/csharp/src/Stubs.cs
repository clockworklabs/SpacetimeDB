using SpacetimeDB.ClientApi;

using System;

namespace SpacetimeDB
{
    public interface IReducerArgsBase : BSATN.IStructuralReadWrite
    {
        string ReducerName { get; }
    }

    public abstract class DbContext<DbView, ReducerView> : DbContext<DbView>
        where DbView : class, new()
        where ReducerView : class, new()
    {
        public ReducerView Reducers;

        public DbContext(DbView db, ReducerView reducers) : base(db) => Reducers = reducers;
    }

    public interface IEventContext {
        bool InvokeHandler();
    }

    public abstract class EventContextBase<RemoteTables, RemoteReducers> : DbContext<RemoteTables, RemoteReducers>, IEventContext
        where RemoteTables : class, new()
        where RemoteReducers : class, new()
    {
        public ulong Timestamp { get; }
        public Identity? Identity { get; }
        public Address? CallerAddress { get; }
        public string? ErrMessage { get; }
        public UpdateStatus? Status { get; }

        public EventContextBase(RemoteTables db, RemoteReducers reducers, TransactionUpdate update) : base(db, reducers)
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

    public abstract class RemoteBase<DbConnection> {
#pragma warning disable CS8618 // Non-nullable field must contain a non-null value when exiting constructor.
        protected DbConnection conn;
#pragma warning restore CS8618

        public void Init(DbConnection conn) {
            this.conn = conn;
        }
    }

    public class RemoteTableHandle<EventContext, T> where EventContext : class, IEventContext {
        public event Action<EventContext, T>? OnInsert;
        public event Action<EventContext, T>? OnDelete;
        public event Action<EventContext, T, T>? OnUpdate;
    }
}
