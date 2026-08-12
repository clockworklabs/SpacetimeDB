using System;
using System.Collections.Generic;

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

    public static class EventListenersProvider
    {
        private static IEventListenersFactory? CustomFactory { get; set; }

        public static IEventListeners<T> Create<T>() where T : Delegate => CustomFactory?.Create<T>() ?? new BasicEventListeners<T>();
        
        public static void SetFactory(IEventListenersFactory factory)
        {
            if(CustomFactory != null) throw new InvalidOperationException("EventListenersFactory can only be set once.");
            CustomFactory = factory;
        }
    }

    public class BasicEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private const int SmallListenerThreshold = 8;
        private const int CollisionBucket = -1;

        private List<T> List { get; } = new(4);
        private Dictionary<int, int>? Indices { get; set; }
        private Dictionary<int, List<int>>? Collisions { get; set; }
        private Stack<List<int>>? CollisionsPool { get; set; }

        public int Count => List.Count;

        public T this[int index] => List[index];

        public void Add(T listener)
        {
            if (listener == null) return;

            var hashCode = listener.GetHashCode();

            if (List.Count <= SmallListenerThreshold)
            {
                if (FindLinear(listener) >= 0) return;

                List.Add(listener);

                if (List.Count > SmallListenerThreshold)
                {
                    RebuildIndex();
                }

                return;
            }

            var indices = Indices!;

            if (!indices.TryGetValue(hashCode, out var index))
            {
                indices.Add(hashCode, List.Count);
                List.Add(listener);
                return;
            }

            if (index != CollisionBucket)
            {
                if (DelegateEquals(List[index], listener)) return;

                var newIndex = List.Count;
                List.Add(listener);
                Collisions ??= new Dictionary<int, List<int>>();
                Collisions[hashCode] = GetCollisionsListFromPool(index, newIndex);
                indices[hashCode] = CollisionBucket;
                return;
            }

            var bucket = Collisions![hashCode];

            for (var i = 0; i < bucket.Count; i++)
            {
                if (DelegateEquals(List[bucket[i]], listener)) return;
            }

            bucket.Add(List.Count);
            List.Add(listener);
        }
        public void Remove(T listener)
        {
            if (listener == null || List.Count <= 0) return;

            var hashCode = listener.GetHashCode();

            if (List.Count <= SmallListenerThreshold)
            {
                var index = FindLinear(listener);
                if (index >= 0)
                {
                    List.RemoveAtSwapBack(index);
                    ClearIndex();
                }

                return;
            }

            var indices = Indices!;

            if (!indices.TryGetValue(hashCode, out var mappedIndex)) return;

            var removeIndex = -1;

            if (mappedIndex != CollisionBucket)
            {
                if (!DelegateEquals(List[mappedIndex], listener)) return;

                removeIndex = mappedIndex;
                indices.Remove(hashCode);
            }
            else
            {
                var bucket = Collisions![hashCode];

                for (var i = 0; i < bucket.Count; i++)
                {
                    var candidate = bucket[i];
                    if (!DelegateEquals(List[candidate], listener)) continue;

                    removeIndex = candidate;
                    RemoveBucketSlot(bucket, i);

                    if (bucket.Count == 1)
                    {
                        indices[hashCode] = bucket[0];
                        Collisions.Remove(hashCode);
                        ReturnCollisionsListToPool(bucket);
                    }
                    else if (bucket.Count == 0)
                    {
                        indices.Remove(hashCode);
                        Collisions.Remove(hashCode);
                        ReturnCollisionsListToPool(bucket);
                    }

                    break;
                }

                if (removeIndex < 0) return;
            }

            var movedFrom = List.Count - 1;
            var movedHashCode = List[movedFrom].GetHashCode();

            List.RemoveAtSwapBack(removeIndex);

            if (List.Count <= SmallListenerThreshold)
            {
                ClearIndex();
            }
            else if (removeIndex != movedFrom)
            {
                UpdateMovedIndex(movedHashCode, movedFrom, removeIndex);
            }
        }

        private int FindLinear(T listener)
        {
            for (var i = 0; i < List.Count; i++)
            {
                if (DelegateEquals(List[i], listener)) return i;
            }

            return -1;
        }

        private void RebuildIndex()
        {
            if (Indices == null)
            {
                Indices = new Dictionary<int, int>(List.Count);
            }
            else
            {
                ClearIndex();
            }

            for (var i = 0; i < List.Count; i++)
            {
                var hashCode = List[i].GetHashCode();

                if (!Indices.TryGetValue(hashCode, out var existing))
                {
                    Indices.Add(hashCode, i);
                    continue;
                }

                Collisions ??= new Dictionary<int, List<int>>();

                if (existing != CollisionBucket)
                {
                    Collisions[hashCode] = GetCollisionsListFromPool(existing, i);
                    Indices[hashCode] = CollisionBucket;
                }
                else
                {
                    Collisions[hashCode].Add(i);
                }
            }
        }

        private void UpdateMovedIndex(int hashCode, int oldIndex, int newIndex)
        {
            var mappedIndex = Indices![hashCode];

            if (mappedIndex != CollisionBucket)
            {
                Indices[hashCode] = newIndex;
                return;
            }

            var bucket = Collisions![hashCode];

            for (var i = 0; i < bucket.Count; i++)
            {
                if (bucket[i] == oldIndex)
                {
                    bucket[i] = newIndex;
                    return;
                }
            }
        }

        private static void RemoveBucketSlot(List<int> bucket, int slot)
        {
            var lastSlot = bucket.Count - 1;

            if (slot != lastSlot)
            {
                bucket[slot] = bucket[lastSlot];
            }

            bucket.RemoveAt(lastSlot);
        }

        private void ClearIndex()
        {
            Indices?.Clear();

            if (Collisions == null) return;

            foreach (var collisions in Collisions.Values)
            {
                ReturnCollisionsListToPool(collisions);
            }

            Collisions.Clear();
        }

        private List<int> GetCollisionsListFromPool(int a, int b)
        {
            if (CollisionsPool == null || CollisionsPool.Count <= 0) return new List<int>(2) { a, b };

            var list = CollisionsPool.Pop();
            list.Add(a);
            list.Add(b);
            return list;
        }

        private void ReturnCollisionsListToPool(List<int> list)
        {
            list.Clear();
            CollisionsPool ??= new Stack<List<int>>();
            CollisionsPool.Push(list);
        }

        private static bool DelegateEquals(T a, T b) => EqualityComparer<T>.Default.Equals(a, b);
    }
}
