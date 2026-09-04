using System;

namespace SpacetimeDB.EventHandling
{
    public interface IEventListeners<T> where T : Delegate
    {
        int Count { get; }
        T this[int index] { get; }

        void Add(T listener);
        void Remove(T listener);
    }

    public interface IEventListenersFactory
    {
        IEventListeners<T> Create<T>() where T : Delegate;
    }
}
