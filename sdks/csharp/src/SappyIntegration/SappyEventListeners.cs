#if SAPPY
using System;
using Sappy;
using SpacetimeDB.EventHandling;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private SapDelegate<T> Targets { get; } = new();
        private DelegateIndex<T, SapTarget<T>> Cache { get; } = new(4);

        public void Add(SapTarget<T> listener) => Targets.Add(listener);
        public void Remove(SapTarget<T> listener) => Targets.Remove(listener);

        public int Count => Targets.Count;
        
        public T this[int index] => Targets[index];

        public void Add(T listener)
        {
            if (listener == null) return;
            if (Cache.Contains(listener)) return;

            var target = new SapTarget<T>(listener);
            Cache.AddUnchecked(listener, target);
            Add(target);
        }

        public void Remove(T listener)
        {
            if (listener == null) return;
            if (Cache.Remove(listener, out var target))
            {
                Remove(target);
            }
        }

        public static SappyEventListeners<T> operator +(SappyEventListeners<T> a, SapTarget<T> b)
        {
            a.Add(b);
            return a;
        }
        public static SappyEventListeners<T> operator -(SappyEventListeners<T> a, SapTarget<T> b)
        {
            a.Remove(b);
            return a;
        }
        public static SappyEventListeners<T> operator +(SappyEventListeners<T> a, T b)
        {
            a.Add(b);
            return a;
        }
        public static SappyEventListeners<T> operator -(SappyEventListeners<T> a, T b)
        {
            a.Remove(b);
            return a;
        }
    }
}
#endif
