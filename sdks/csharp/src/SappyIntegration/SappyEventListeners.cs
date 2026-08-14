#if SAPPY
using System;
using System.Collections.Generic;
using Sappy;
using SpacetimeDB.EventHandling;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private SapDelegate<T> Targets { get; } = new();
        private TargetCache Cache { get; } = new(4);

        public void Add(SapTarget<T> listener) => Targets.Add(listener);
        public void Remove(SapTarget<T> listener) => Targets.Remove(listener);

        public int Count => Targets.Count;
        
        public T this[int index] => Targets[index];

        public void Add(T listener)
        {
            if (listener == null) return;
            if (Cache.Add(listener, out var target))
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

        private sealed class TargetCache
        {
            private const int SmallListenerThreshold = 8;
            private const int CollisionBucket = -1;

            private int[] Hashes;
            private T?[] Callbacks;
            private SapTarget<T>?[] Targets;
            private int Capacity { get; set; }
            private int Count { get; set; }
            private Dictionary<int, int>? Indices { get; set; }
            private Dictionary<int, List<int>>? Collisions { get; set; }
            private Stack<List<int>>? CollisionsPool { get; set; }

            public TargetCache(int capacity)
            {
                Capacity = Math.Max(1, capacity);
                Hashes = new int[Capacity];
                Callbacks = new T[Capacity];
                Targets = new SapTarget<T>[Capacity];
            }

            public bool Add(T callback, out SapTarget<T> target)
            {
                target = null!;
                if (callback == null) return false;

                var hashCode = callback.GetHashCode();

                if (Count <= SmallListenerThreshold)
                {
                    if (FindLinear(callback) >= 0) return false;

                    target = new SapTarget<T>(callback);
                    AddRaw(hashCode, callback, target);

                    if (Count > SmallListenerThreshold)
                    {
                        RebuildIndex();
                    }

                    return true;
                }

                var indices = Indices!;

                if (!indices.TryGetValue(hashCode, out var index))
                {
                    target = new SapTarget<T>(callback);
                    indices.Add(hashCode, AddRaw(hashCode, callback, target));
                    return true;
                }

                if (index != CollisionBucket)
                {
                    if (DelegateEquals(Callbacks[index]!, callback)) return false;

                    target = new SapTarget<T>(callback);
                    var newIndex = AddRaw(hashCode, callback, target);
                    Collisions ??= new Dictionary<int, List<int>>();
                    Collisions[hashCode] = GetCollisionsListFromPool(index, newIndex);
                    indices[hashCode] = CollisionBucket;
                    return true;
                }

                var bucket = Collisions![hashCode];

                for (var i = 0; i < bucket.Count; i++)
                {
                    if (DelegateEquals(Callbacks[bucket[i]]!, callback)) return false;
                }

                target = new SapTarget<T>(callback);
                bucket.Add(AddRaw(hashCode, callback, target));
                return true;
            }

            public bool Remove(T callback, out SapTarget<T> target)
            {
                target = null!;
                if (callback == null || Count <= 0) return false;

                var hashCode = callback.GetHashCode();

                if (Count <= SmallListenerThreshold)
                {
                    var index = FindLinear(callback);
                    if (index >= 0)
                    {
                        target = Targets[index]!;
                        RemoveAtSwapBackRaw(index);
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
                    if (!DelegateEquals(Callbacks[mappedIndex]!, callback)) return false;

                    removeIndex = mappedIndex;
                    indices.Remove(hashCode);
                }
                else
                {
                    var bucket = Collisions![hashCode];

                    for (var i = 0; i < bucket.Count; i++)
                    {
                        var candidate = bucket[i];
                        if (!DelegateEquals(Callbacks[candidate]!, callback)) continue;

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

                var movedFrom = Count - 1;
                var movedHashCode = Hashes[movedFrom];
                target = Targets[removeIndex]!;

                RemoveAtSwapBackRaw(removeIndex);

                if (Count <= SmallListenerThreshold)
                {
                    ClearIndex();
                }
                else if (removeIndex != movedFrom)
                {
                    UpdateMovedIndex(movedHashCode, movedFrom, removeIndex);
                }

                return true;
            }

            private int FindLinear(T callback)
            {
                for (var i = 0; i < Count; i++)
                {
                    if (DelegateEquals(Callbacks[i]!, callback)) return i;
                }

                return -1;
            }

            private int AddRaw(int hashCode, T callback, SapTarget<T> target)
            {
                EnsureCapacity();

                var index = Count;
                Hashes[index] = hashCode;
                Callbacks[index] = callback;
                Targets[index] = target;
                Count++;
                return index;
            }

            private void RemoveAtSwapBackRaw(int index)
            {
                var lastIndex = Count - 1;

                if (index != lastIndex)
                {
                    Hashes[index] = Hashes[lastIndex];
                    Callbacks[index] = Callbacks[lastIndex];
                    Targets[index] = Targets[lastIndex];
                }

                Hashes[lastIndex] = 0;
                Callbacks[lastIndex] = null;
                Targets[lastIndex] = null;
                Count = lastIndex;
            }

            private void EnsureCapacity()
            {
                if (Count < Capacity) return;

                Capacity *= 2;
                Array.Resize(ref Hashes, Capacity);
                Array.Resize(ref Callbacks, Capacity);
                Array.Resize(ref Targets, Capacity);
            }

            private void RebuildIndex()
            {
                if (Indices == null)
                {
                    Indices = new Dictionary<int, int>(Capacity);
                }
                else
                {
                    ClearIndex();
                }

                for (var i = 0; i < Count; i++)
                {
                    var hashCode = Hashes[i];

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
}
#endif
