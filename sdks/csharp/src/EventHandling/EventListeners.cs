using System;
using System.Collections.Generic;

namespace SpacetimeDB.EventHandling
{
    public class EventListeners<T> where T : Delegate
    {
#if SAPPY
        public SapDelegate<T> Targets { get; } = new();
        private Dictionary<int, SapTarget<T>> Cache { get; } = new(4);

        public void Add(SapTarget<T> listener) => Targets.Add(listener);
        public void Remove(SapTarget<T> listener) => Targets.Remove(listener);

        public int Count => Targets.Count;
        
        public T this[int index] => Targets[index];

        public void Add(T listener)
        {
            if(listener == null) return;
            var hashCode = listener.GetHashCode();
            if(!Cache.TryGetValue(hashCode, out var target)) {
                target = new SapTarget<T>(listener);
                Cache.Add(hashCode, target);
            }
            Add(target);
        }
        public void Remove(T listener)
        {
            if(listener == null || !Cache.TryGetValue(listener.GetHashCode(), out var target)) return;
            Remove(target);
        }

        public static EventListeners<T> operator +(EventListeners<T> a, SapTarget<T> b)
        {
            a.Add(b);
            return a;
        }
        public static EventListeners<T> operator -(EventListeners<T> a, SapTarget<T> b)
        {
            a.Remove(b);
            return a;
        }
        public static EventListeners<T> operator +(EventListeners<T> a, T b)
        {
            a.Add(b);
            return a;
        }
        public static EventListeners<T> operator -(EventListeners<T> a, T b)
        {
            a.Remove(b);
            return a;
        }
#else
        private List<T> List { get; } = new(4);
        private Dictionary<int, int> Indices { get; } = new(4);

        public int Count => List.Count;

        public T this[int index] => List[index];

        public void Add(T listener)
        {
            if (listener == null || !Indices.TryAdd(listener.GetHashCode(), List.Count)) return;
            List.Add(listener);
        }
        public void Remove(T listener)
        {
            if (listener == null || List.Count <= 0) return;
            var hashCode = listener.GetHashCode();
            if(!Indices.Remove(hashCode, out var index)) return;
            var lastListener = List[^1];
            var lastListenerHashCode = lastListener.GetHashCode();
            if (lastListenerHashCode != hashCode)
            {
                Indices[lastListenerHashCode] = index;
            }
            List.RemoveAtSwapBack(index);
        }
#endif
    }
}