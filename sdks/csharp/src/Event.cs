using System;
using System.Collections.Generic;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public interface IEventContext
    {

    }

    public interface IReducerEventContext
    {

    }

    public interface ISubscriptionEventContext
    {

    }

    public interface IErrorContext
    {
        public Exception Event { get; }
    }

    // The following underscores are needed to work around c#'s unified type-and-function
    // namespace.

    /// <summary>
    /// The <c>DbContext</c> trait, which mediates access to a remote module.
    ///
    /// <c>DbContext</c> is implemented by <c>DbConnection</c> and <c>EventContext</c>,
    /// both defined in your module-specific codegen.
    /// </summary>
    public interface IDbContext<DbView, RemoteReducers, SetReducerFlags_, SubscriptionBuilder_>
    {
        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public DbView Db { get; }

        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers { get; }

        /// <summary>
        /// Access to setters for per-reducer flags.
        ///
        /// The returned <c>SetReducerFlags</c> will have a method to invoke,
        /// for each reducer defined by the module,
        /// which call-flags for the reducer can be set.
        /// </summary>
        public SetReducerFlags_ SetReducerFlags { get; }

        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive { get; }

        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect();

        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder_ SubscriptionBuilder();

        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity { get; }

        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId { get; }
    }

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
        Timestamp Timestamp,
        Status Status,
        Identity CallerIdentity,
        ConnectionId? CallerConnectionId,
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


    public interface ISubscriptionHandle
    {
        void OnApplied(ISubscriptionEventContext ctx, SubscriptionAppliedType state);
        void OnError(IErrorContext ctx);
        void OnEnded(ISubscriptionEventContext ctx);
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
    public partial record SubscriptionState
        : TaggedEnum<(Unit Pending, QueryId Active, Unit LegacyActive, Unit Ended)>
    { }

    public class SubscriptionHandleBase<SubscriptionEventContext, ErrorContext> : ISubscriptionHandle
        where SubscriptionEventContext : ISubscriptionEventContext
        where ErrorContext : IErrorContext
    {
        private readonly IDbConnection conn;
        private readonly Action<SubscriptionEventContext>? onApplied;
        private readonly Action<ErrorContext, Exception>? onError;
        private Action<SubscriptionEventContext>? onEnded;

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

        void ISubscriptionHandle.OnApplied(ISubscriptionEventContext ctx, SubscriptionAppliedType type)
        {
            if (type is SubscriptionAppliedType.Active active)
            {
                state = new SubscriptionState.Active(active.Active_);
            }
            else if (type is SubscriptionAppliedType.LegacyActive)
            {
                state = new SubscriptionState.LegacyActive(new());
            }
            onApplied?.Invoke((SubscriptionEventContext)ctx);
        }

        void ISubscriptionHandle.OnEnded(ISubscriptionEventContext ctx)
        {
            state = new SubscriptionState.Ended(new());
            onEnded?.Invoke((SubscriptionEventContext)ctx);
        }

        void ISubscriptionHandle.OnError(IErrorContext ctx)
        {
            state = new SubscriptionState.Ended(new());
            onError?.Invoke((ErrorContext)ctx, ctx.Event);
        }

        /// <summary>
        /// Construct a legacy subscription handle.
        /// </summary>
        /// <param name="conn"></param>
        /// <param name="onApplied"></param>
        /// <param name="onError"></param>
        /// <param name="querySqls"></param>
        protected SubscriptionHandleBase(IDbConnection conn, Action<SubscriptionEventContext>? onApplied, string[] querySqls)
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
        protected SubscriptionHandleBase(
            IDbConnection conn,
            Action<SubscriptionEventContext>? onApplied,
            Action<ErrorContext, Exception>? onError,
            string[] querySqls
        )
        {
            state = new SubscriptionState.Pending(new());
            this.onApplied = onApplied;
            this.onError = onError;
            this.conn = conn;
            conn.Subscribe(this, querySqls);
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
        public void UnsubscribeThen(Action<SubscriptionEventContext>? onEnded)
        {
            if (state is not SubscriptionState.Active)
            {
                throw new Exception("Cannot unsubscribe from inactive subscription.");
            }
            if (this.onEnded != null)
            {
                throw new Exception("Unsubscribe already called.");
            }
            if (onEnded == null)
            {
                // We need to put something in there to use this as a boolean.
                onEnded = (ctx) => { };
            }
            this.onEnded = onEnded;
        }
    }
}
