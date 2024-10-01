using System;

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
        private event Action<EventContext>? Applied;
        private event Action<EventContext>? Error;

        public SubscriptionBuilder(IDbConnection conn)
        {
            this.conn = conn;
        }

        public SubscriptionBuilder<EventContext> OnApplied(Action<EventContext> callback)
        {
            Applied += callback;
            return this;
        }

        public SubscriptionBuilder<EventContext> OnError(Action<EventContext> callback)
        {
            Error += callback;
            return this;
        }

        public SubscriptionHandle<EventContext> Subscribe(string querySql) => new(conn, Applied, Error, querySql);
    }

    public interface ISubscriptionHandle
    {
        void OnApplied(IEventContext ctx);
    }

    public class SubscriptionHandle<EventContext> : ISubscriptionHandle
        where EventContext : IEventContext
    {
        private readonly Action<EventContext>? onApplied;

        void ISubscriptionHandle.OnApplied(IEventContext ctx)
        {
            IsActive = true;
            onApplied?.Invoke((EventContext)ctx);
        }

        internal SubscriptionHandle(IDbConnection conn, Action<EventContext>? onApplied, Action<EventContext>? onError, string querySql)
        {
            this.onApplied = onApplied;
            conn.Subscribe(this, querySql);
        }

        public void Unsubscribe() => throw new NotImplementedException();

        public void UnsuscribeThen(Action<EventContext> onEnd) => throw new NotImplementedException();

        public bool IsEnded => false;
        public bool IsActive { get; private set; }
    }
}
