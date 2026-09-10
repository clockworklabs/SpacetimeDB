using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

namespace SpacetimeDB.EventHandling
{
    internal sealed class EventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private const int SmallListenerThreshold = 8;
        private const int CollisionBucket = -1;

        private static readonly EqualityComparer<T> Comparer = EqualityComparer<T>.Default;

        private int[] _hashes;
        private T?[] _listeners;
        private int _count;
        private Dictionary<int, int>? _indices;
        private Dictionary<int, List<int>>? _collisions;
        private Stack<List<int>>? _collisionsPool;

        public int Count
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => _count;
        }

        public T this[int index]
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => _listeners[index]!;
        }

        public EventListeners() : this(4) { }

        public EventListeners(int initialSize)
        {
            var capacity = Math.Max(1, initialSize);
            _hashes = new int[capacity];
            _listeners = new T[capacity];
        }

        public void Add(T listener)
        {
            if (listener == null) return;

            var hashCode = listener.GetHashCode();

            if (_count <= SmallListenerThreshold)
            {
                AddRaw(hashCode, listener);

                if (_count > SmallListenerThreshold)
                {
                    RebuildIndex();
                }

                return;
            }

            var newIndex = AddRaw(hashCode, listener);
            var indices = _indices!;

            if (!indices.TryGetValue(hashCode, out var index))
            {
                indices.Add(hashCode, newIndex);
                return;
            }

            if (index != CollisionBucket)
            {
                _collisions ??= new Dictionary<int, List<int>>();
                _collisions[hashCode] = GetCollisionsListFromPool(index, newIndex);
                indices[hashCode] = CollisionBucket;
                return;
            }

            var bucket = _collisions![hashCode];
            bucket.Add(newIndex);
        }

        public void Remove(T listener)
        {
            if (listener == null || _count <= 0) return;

            if (_count <= SmallListenerThreshold)
            {
                var index = FindLinear(listener);
                if (index >= 0)
                {
                    RemoveAtSwapBackRaw(index);
                }

                return;
            }

            var hashCode = listener.GetHashCode();
            var indices = _indices;
            if (indices == null) return;

            if (!indices.TryGetValue(hashCode, out var mappedIndex)) return;

            var removeIndex = -1;

            if (mappedIndex != CollisionBucket)
            {
                if (!DelegateEquals(_listeners[mappedIndex]!, listener)) return;

                removeIndex = mappedIndex;
                indices.Remove(hashCode);
            }
            else
            {
                var bucket = _collisions![hashCode];

                for (var i = 0; i < bucket.Count; i++)
                {
                    var candidate = bucket[i];
                    if (!DelegateEquals(_listeners[candidate]!, listener)) continue;

                    removeIndex = candidate;
                    RemoveBucketSlot(bucket, i);

                    if (bucket.Count == 1)
                    {
                        indices[hashCode] = bucket[0];
                        _collisions.Remove(hashCode);
                        ReturnCollisionsListToPool(bucket);
                    }
                    else if (bucket.Count == 0)
                    {
                        indices.Remove(hashCode);
                        _collisions.Remove(hashCode);
                        ReturnCollisionsListToPool(bucket);
                    }

                    break;
                }

                if (removeIndex < 0) return;
            }

            var movedFrom = _count - 1;
            var movedHashCode = _hashes[movedFrom];

            RemoveAtSwapBackRaw(removeIndex);

            if (_count <= SmallListenerThreshold)
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
            for (var i = 0; i < _count; i++)
            {
                if (DelegateEquals(_listeners[i]!, listener)) return i;
            }

            return -1;
        }

        private int AddRaw(int hashCode, T listener)
        {
            EnsureCapacity();

            var index = _count;
            _hashes[index] = hashCode;
            _listeners[index] = listener;
            _count++;
            return index;
        }

        private void RemoveAtSwapBackRaw(int index)
        {
            var lastIndex = _count - 1;

            if (index != lastIndex)
            {
                _hashes[index] = _hashes[lastIndex];
                _listeners[index] = _listeners[lastIndex];
            }

            _hashes[lastIndex] = 0;
            _listeners[lastIndex] = null;
            _count = lastIndex;
        }

        private void EnsureCapacity()
        {
            var capacity = _listeners.Length;
            if (_count < capacity) return;

            capacity *= 2;
            Array.Resize(ref _hashes, capacity);
            Array.Resize(ref _listeners, capacity);
        }

        private void UpdateMovedIndex(int hashCode, int oldIndex, int newIndex)
        {
            var indices = _indices!;
            var mappedIndex = indices[hashCode];

            if (mappedIndex != CollisionBucket)
            {
                indices[hashCode] = newIndex;
                return;
            }

            var bucket = _collisions![hashCode];

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

        private void RebuildIndex()
        {
            if (_indices == null)
            {
                _indices = new Dictionary<int, int>(_listeners.Length);
            }
            else
            {
                ClearIndex();
            }

            var indices = _indices;
            for (var i = 0; i < _count; i++)
            {
                var hashCode = _hashes[i];

                if (!indices.TryGetValue(hashCode, out var existing))
                {
                    indices.Add(hashCode, i);
                    continue;
                }

                _collisions ??= new Dictionary<int, List<int>>();

                if (existing != CollisionBucket)
                {
                    _collisions[hashCode] = GetCollisionsListFromPool(existing, i);
                    indices[hashCode] = CollisionBucket;
                }
                else
                {
                    _collisions[hashCode].Add(i);
                }
            }
        }

        private void ClearIndex()
        {
            _indices?.Clear();

            if (_collisions == null) return;

            foreach (var collisions in _collisions.Values)
            {
                ReturnCollisionsListToPool(collisions);
            }

            _collisions.Clear();
        }

        private List<int> GetCollisionsListFromPool(int a, int b)
        {
            if (_collisionsPool == null || _collisionsPool.Count <= 0) return new List<int>(2) { a, b };

            var list = _collisionsPool.Pop();
            list.Add(a);
            list.Add(b);
            return list;
        }

        private void ReturnCollisionsListToPool(List<int> list)
        {
            list.Clear();
            _collisionsPool ??= new Stack<List<int>>();
            _collisionsPool.Push(list);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static bool DelegateEquals(T a, T b) => Comparer.Equals(a, b);
    }
}
