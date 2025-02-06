using System;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public interface IEventContext { }

    public interface IReducerArgs : BSATN.IStructuralReadWrite
    {
        string ReducerName { get; }
    }

    [Type]
    public partial record Status : TaggedEnum<(
        Unit Committed,
        string Failed,
        Unit OutOfEnergy
    )>;

    public record ReducerEvent<R>(
        DateTimeOffset Timestamp,
        Status Status,
        Identity CallerIdentity,
        Address? CallerAddress,
        U128? EnergyConsumed,
        R Reducer
    );

    public record Event<R>
    {
        private Event() { }

        public record Reducer(ReducerEvent<R> ReducerEvent) : Event<R>;
        public record SubscribeApplied : Event<R>;
        public record UnsubscribeApplied : Event<R>;
        public record SubscribeError(Exception Exception) : Event<R>;
        public record UnknownTransaction : Event<R>;
    }

    // TODO: Move those classes into EventContext, so that we wouldn't need repetitive generics.
    public sealed class SubscriptionBuilder<EventContext>
        where EventContext : IEventContext
    {
        private readonly IDbConnection conn;
        public delegate void Callback(EventContext ctx);
        private event Callback? Applied;
        private event Callback? Error;

        public SubscriptionBuilder(IDbConnection conn)
        {
            this.conn = conn;
        }

        public SubscriptionBuilder<EventContext> OnApplied(Callback callback)
        {
            Applied += callback;
            return this;
        }

        public SubscriptionBuilder<EventContext> OnError(Callback callback)
        {
            Error += callback;
            return this;
        }

        public SubscriptionHandle<EventContext> Subscribe(string querySql) => new(conn, Applied, Error, querySql);

        public void SubscribeToAllTables()
        {
            // Make sure we use the legacy handle constructor here, even though there's only 1 query.
            // We drop the error handler, since it can't be called for legacy subscriptions.
            new SubscriptionHandle<EventContext>(conn, Applied, new string[] { "SELECT * FROM *" });
        }
    }

    public interface ISubscriptionHandle
    {
        void OnApplied(IEventContext ctx, SubscriptionAppliedType state);
        void OnError(IEventContext ctx);
        void OnEnded(IEventContext ctx);
    }

    /// <summary>
    /// An applied subscription can either be a new-style subscription (with a query ID),
    /// or a legacy subscription (no query ID).
    /// </summary>
    [Type]
    public partial record SubscriptionAppliedType : TaggedEnum<(
        QueryId Active,
        Unit LegacyActive)>
    { }

    /// <summary>
    /// State flow chart:
    /// <c>
    ///           |
    ///           v
    ///        Pending
    ///        |     |
    ///        v     v
    ///     Active  LegacyActive
    ///        |
    ///        v
    ///     Ended
    /// </c>
    /// </summary>
    [Type]
    public partial record SubscriptionState : TaggedEnum<(
        Unit Pending,
        QueryId Active,
        Unit LegacyActive,
        Unit Ended)>
    { }

    public class SubscriptionHandle<EventContext> : ISubscriptionHandle
        where EventContext : IEventContext
    {
        private readonly IDbConnection conn;
        private readonly SubscriptionBuilder<EventContext>.Callback? onApplied;
        private readonly SubscriptionBuilder<EventContext>.Callback? onError;
        private SubscriptionBuilder<EventContext>.Callback? onEnded;

        private SubscriptionState state;

        /// <summary>
        /// Whether the subscription has ended.
        /// </summary>
        public bool IsEnded
        {
            get
            {
                return state is SubscriptionState.Ended;
            }
        }

        /// <summary>
        /// Whether the subscription is active.
        /// </summary>
        public bool IsActive
        {
            get
            {
                return state is SubscriptionState.Active || state is SubscriptionState.LegacyActive;
            }
        }

        void ISubscriptionHandle.OnApplied(IEventContext ctx, SubscriptionAppliedType type)
        {
            if (type is SubscriptionAppliedType.Active active)
            {
                state = new SubscriptionState.Active(active.Active_);
            }
            else if (type is SubscriptionAppliedType.LegacyActive)
            {
                state = new SubscriptionState.LegacyActive(new());
            }
            onApplied?.Invoke((EventContext)ctx);
        }

        void ISubscriptionHandle.OnEnded(IEventContext ctx)
        {
            state = new SubscriptionState.Ended(new());
            onEnded?.Invoke((EventContext)ctx);
        }

        void ISubscriptionHandle.OnError(IEventContext ctx)
        {
            state = new SubscriptionState.Ended(new());
            onError?.Invoke((EventContext)ctx);
        }

        /// <summary>
        /// TODO: remove this constructor once legacy subscriptions are removed.
        /// </summary>
        /// <param name="conn"></param>
        /// <param name="onApplied"></param>
        /// <param name="onError"></param>
        /// <param name="querySqls"></param>
        internal SubscriptionHandle(IDbConnection conn, SubscriptionBuilder<EventContext>.Callback? onApplied, string[] querySqls)
        {
            state = new SubscriptionState.Pending(new());
            this.conn = conn;
            this.onApplied = onApplied;
            conn.LegacySubscribe(this, querySqls);
        }

        /// <summary>
        /// Construct a subscription handle.
        /// </summary>
        /// <param name="conn"></param>
        /// <param name="onApplied"></param>
        /// <param name="onError"></param>
        /// <param name="querySql"></param>
        internal SubscriptionHandle(IDbConnection conn, SubscriptionBuilder<EventContext>.Callback? onApplied, SubscriptionBuilder<EventContext>.Callback? onError, string querySql)
        {
            state = new SubscriptionState.Pending(new());
            this.onApplied = onApplied;
            this.onError = onError;
            this.conn = conn;
            conn.Subscribe(this, querySql);
        }

        /// <summary>
        /// Unsubscribe from the query controlled by this subscription handle.
        /// 
        /// Calling this more than once will result in an exception.
        /// </summary>
        public void Unsubscribe()
        {
            UnsubscribeThen(null);
        }

        /// <summary>
        /// Unsubscribe from the query controlled by this subscription handle,
        /// and call onEnded when its rows are removed from the client cache.
        /// </summary>
        public void UnsubscribeThen(SubscriptionBuilder<EventContext>.Callback? onEnded)
        {
            if (state is not SubscriptionState.Active)
            {
                throw new Exception("Cannot unsubscribe from inactive subscription.");
            }
            if (onEnded != null)
            {
                // TODO: should we just log here instead? Do we try not to throw exceptions on the main thread?
                throw new Exception("Unsubscribe already called.");
            }
            this.onEnded = onEnded;
        }
    }
}
