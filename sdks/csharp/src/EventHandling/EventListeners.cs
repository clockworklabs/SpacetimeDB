using System;
using System.Collections.Generic;

namespace SpacetimeDB.EventHandling
{
    internal class EventListeners<T> where T : Delegate
    {
        private List<T> List { get; }
        private Dictionary<T, int> Indices { get; }

        public int Count => List.Count;

        public T this[int index] => List[index];

        public EventListeners() : this(0) { }
        public EventListeners(int initialSize)
        {
            List = new List<T>(initialSize);
            Indices = new Dictionary<T, int>(initialSize);
        }

        public void Add(T listener)
        {
            if (listener == null || !Indices.TryAdd(listener, List.Count)) return;
            List.Add(listener);
        }

        public void Remove(T listener)
        {
            if (listener == null || List.Count <= 0 || !Indices.Remove(listener, out var index)) return;
            var lastListener = List[^1];
            if (lastListener != listener)
            {
                Indices[lastListener] = index;
            }

            List.RemoveAtSwapBack(index);
        }
    }
}