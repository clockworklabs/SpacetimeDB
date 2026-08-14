#if SAPPY
using System;
using Sappy;
using SpacetimeDB.EventHandling;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private static Func<T, SappyEventListeners<T>, SapTarget<T>> CreateTarget { get; } = CreateTargetFromListener;

        private SapDelegate<T> Targets { get; } = new();
        private DelegateIndex<T, SapTarget<T>> Cache { get; } = new(4);

        public void Add(SapTarget<T> listener) => Targets.Add(listener);
        public void Remove(SapTarget<T> listener) => Targets.Remove(listener);

        public int Count => Targets.Count;
        
        public T this[int index] => Targets[index];

        public void Add(T listener)
        {
            if (listener == null) return;
            if (Cache.Add(listener, this, CreateTarget, out var target))
            {
                Add(target);
            }
        }

        public void Remove(T listener)
        {
            if (listener == null) return;
            if (Cache.Remove(listener, out var target))
            {
                Remove(target);
            }
        }

        private static SapTarget<T> CreateTargetFromListener(T listener, SappyEventListeners<T> _) => new(listener);
    }
}
#endif
