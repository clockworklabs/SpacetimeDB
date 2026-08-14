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
        
        public static IEventListeners<T> operator +(IEventListeners<T> a, T b)
        {
            a.Add(b);
            return a;
        }
        public static IEventListeners<T> operator -(IEventListeners<T> a, T b)
        {
            a.Remove(b);
            return a;
        }
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
            CustomFactory = factory;
        }
    }

    public class BasicEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private DelegateIndex<T, T> Listeners { get; } = new(4);

        public int Count => Listeners.Count;

        public T this[int index] => Listeners[index];

        public void Add(T listener)
        {
            if (listener == null) return;
            Listeners.Add(listener, listener, static (_, listener) => listener);
        }

        public void Remove(T listener)
        {
            if (listener == null) return;
            Listeners.Remove(listener, out _);
        }
    }

    public class DelegateIndex<TDelegate, TValue> where TDelegate : Delegate
    {
        private const int SmallListenerThreshold = 8;
        private const int CollisionBucket = -1;

        private IEqualityComparer<TDelegate> Comparer { get; }
        private List<TDelegate> Keys { get; }
        private List<TValue> Values { get; }
        private Dictionary<int, int>? Indices { get; set; }
        private Dictionary<int, List<int>>? Collisions { get; set; }
        private Stack<List<int>>? CollisionsPool { get; set; }

        public int Count => Values.Count;

        public TValue this[int index] => Values[index];

        public DelegateIndex() : this(0) { }

        public DelegateIndex(int initialSize) : this(initialSize, EqualityComparer<TDelegate>.Default) { }

        public DelegateIndex(int initialSize, IEqualityComparer<TDelegate> comparer)
        {
            Comparer = comparer;
            Keys = new List<TDelegate>(initialSize);
            Values = new List<TValue>(initialSize);
        }

        public bool Add(TDelegate key, TValue value)
        {
            return Add(key, value, static (_, value) => value, out _);
        }

        public bool Add<TState>(TDelegate key, TState state, Func<TDelegate, TState, TValue> createValue)
        {
            return Add(key, state, createValue, out _);
        }

        public bool Add<TState>(TDelegate key, TState state, Func<TDelegate, TState, TValue> createValue, out TValue value)
        {
            value = default!;
            if (key == null) return false;

            var hashCode = Comparer.GetHashCode(key);

            if (Keys.Count <= SmallListenerThreshold)
            {
                if (FindLinear(key) >= 0) return false;

                value = createValue(key, state);
                AddUnchecked(key, value, hashCode);
                return true;
            }

            var indices = Indices!;

            if (!indices.TryGetValue(hashCode, out var index))
            {
                value = createValue(key, state);
                AddUnchecked(key, value, hashCode);
                return true;
            }

            if (index != CollisionBucket)
            {
                if (DelegateEquals(Keys[index], key)) return false;

                value = createValue(key, state);
                AddUnchecked(key, value, hashCode);
                return true;
            }

            var bucket = Collisions![hashCode];

            for (var i = 0; i < bucket.Count; i++)
            {
                if (DelegateEquals(Keys[bucket[i]], key)) return false;
            }

            value = createValue(key, state);
            AddUnchecked(key, value, hashCode);
            return true;
        }

        public bool Contains(TDelegate key)
        {
            if (key == null || Keys.Count <= 0) return false;

            var hashCode = Comparer.GetHashCode(key);

            if (Keys.Count <= SmallListenerThreshold)
            {
                return FindLinear(key) >= 0;
            }

            var indices = Indices!;

            if (!indices.TryGetValue(hashCode, out var index)) return false;

            if (index != CollisionBucket)
            {
                return DelegateEquals(Keys[index], key);
            }

            var bucket = Collisions![hashCode];

            for (var i = 0; i < bucket.Count; i++)
            {
                if (DelegateEquals(Keys[bucket[i]], key)) return true;
            }

            return false;
        }

        public void AddUnchecked(TDelegate key, TValue value)
        {
            var hashCode = Comparer.GetHashCode(key);
            AddUnchecked(key, value, hashCode);
        }

        private void AddUnchecked(TDelegate key, TValue value, int hashCode)
        {
            if (Keys.Count <= SmallListenerThreshold)
            {
                Keys.Add(key);
                Values.Add(value);

                if (Keys.Count > SmallListenerThreshold)
                {
                    RebuildIndex();
                }

                return;
            }

            var indices = Indices!;

            if (!indices.TryGetValue(hashCode, out var index))
            {
                indices.Add(hashCode, Keys.Count);
                Keys.Add(key);
                Values.Add(value);
                return;
            }

            var newIndex = Keys.Count;
            Keys.Add(key);
            Values.Add(value);

            if (index != CollisionBucket)
            {
                Collisions ??= new Dictionary<int, List<int>>();
                Collisions[hashCode] = GetCollisionsListFromPool(index, newIndex);
                indices[hashCode] = CollisionBucket;
                return;
            }

            Collisions![hashCode].Add(newIndex);
        }

        public bool Remove(TDelegate key, out TValue value)
        {
            value = default!;
            if (key == null || Keys.Count <= 0) return false;

            var hashCode = Comparer.GetHashCode(key);

            if (Keys.Count <= SmallListenerThreshold)
            {
                var index = FindLinear(key);
                if (index >= 0)
                {
                    value = Values[index];
                    Keys.RemoveAtSwapBack(index);
                    Values.RemoveAtSwapBack(index);
                    ClearIndex();
                    return true;
                }

                return false;
            }

            var indices = Indices!;

            if (!indices.TryGetValue(hashCode, out var mappedIndex)) return false;

            var removeIndex = -1;

            if (mappedIndex != CollisionBucket)
            {
                if (!DelegateEquals(Keys[mappedIndex], key)) return false;

                removeIndex = mappedIndex;
                indices.Remove(hashCode);
            }
            else
            {
                var bucket = Collisions![hashCode];

                for (var i = 0; i < bucket.Count; i++)
                {
                    var candidate = bucket[i];
                    if (!DelegateEquals(Keys[candidate], key)) continue;

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

                if (removeIndex < 0) return false;
            }

            var movedFrom = Keys.Count - 1;
            var movedHashCode = Comparer.GetHashCode(Keys[movedFrom]);
            value = Values[removeIndex];

            Keys.RemoveAtSwapBack(removeIndex);
            Values.RemoveAtSwapBack(removeIndex);

            if (Keys.Count <= SmallListenerThreshold)
            {
                ClearIndex();
            }
            else if (removeIndex != movedFrom)
            {
                UpdateMovedIndex(movedHashCode, movedFrom, removeIndex);
            }

            return true;
        }

        private int FindLinear(TDelegate key)
        {
            for (var i = 0; i < Keys.Count; i++)
            {
                if (DelegateEquals(Keys[i], key)) return i;
            }

            return -1;
        }

        private void RebuildIndex()
        {
            if (Indices == null)
            {
                Indices = new Dictionary<int, int>(Keys.Count);
            }
            else
            {
                ClearIndex();
            }

            for (var i = 0; i < Keys.Count; i++)
            {
                var hashCode = Comparer.GetHashCode(Keys[i]);

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

        private bool DelegateEquals(TDelegate a, TDelegate b) => Comparer.Equals(a, b);
    }
}
