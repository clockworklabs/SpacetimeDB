#if SAPPY
using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using Sappy;
using SpacetimeDB.EventHandling;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private readonly TargetCache _cache = new(4);

        public void Add(SapTarget<T> listener) => _cache.Add(listener);
        public void Remove(SapTarget<T> listener) => _cache.Remove(listener);

        public int Count => _cache.Count;
        
        public T this[int index] => _cache[index];

        public void Add(T listener)
        {
            if (listener == null) return;
            _cache.Add(listener);
        }

        public void Remove(T listener)
        {
            if (listener == null) return;
            _cache.Remove(listener);
        }

        private sealed class TargetCache
        {
            private const int SmallListenerThreshold = 8;
            private const int CollisionBucket = -1;

            private static readonly EqualityComparer<T> Comparer = EqualityComparer<T>.Default;

            private int[] _hashes;
            private T?[] _callbacks;
            private SapTarget<T>?[] _targets;
            private int _capacity;
            private int _count;
            private Dictionary<int, int>? _indices;
            private Dictionary<int, List<int>>? _collisions;
            private Stack<List<int>>? _collisionsPool;

            public int Count => _count;

            public T this[int index] => _callbacks[index]!;

            public TargetCache(int capacity)
            {
                _capacity = Math.Max(1, capacity);
                _hashes = new int[_capacity];
                _callbacks = new T[_capacity];
                _targets = new SapTarget<T>[_capacity];
            }

            public bool Add(T callback)
            {
                if (callback == null) return false;

                var hashCode = callback.GetHashCode();

                if (_count <= SmallListenerThreshold)
                {
                    if (FindLinear(callback) >= 0) return false;

                    AddRaw(hashCode, callback, new SapTarget<T>(callback));

                    if (_count > SmallListenerThreshold)
                    {
                        RebuildIndex();
                    }

                    return true;
                }

                var indices = _indices!;

                if (!indices.TryGetValue(hashCode, out var index))
                {
                    indices.Add(hashCode, AddRaw(hashCode, callback, new SapTarget<T>(callback)));
                    return true;
                }

                if (index != CollisionBucket)
                {
                    if (DelegateEquals(_callbacks[index]!, callback)) return false;

                    var newIndex = AddRaw(hashCode, callback, new SapTarget<T>(callback));
                    _collisions ??= new Dictionary<int, List<int>>();
                    _collisions[hashCode] = GetCollisionsListFromPool(index, newIndex);
                    indices[hashCode] = CollisionBucket;
                    return true;
                }

                var bucket = _collisions![hashCode];

                for (var i = 0; i < bucket.Count; i++)
                {
                    if (DelegateEquals(_callbacks[bucket[i]]!, callback)) return false;
                }

                bucket.Add(AddRaw(hashCode, callback, new SapTarget<T>(callback)));
                return true;
            }

            public bool Add(SapTarget<T> target)
            {
                if (target == null || target.Callback == null) return false;

                var hashCode = target.HashCode;
                var callback = target.Callback;

                if (_count <= SmallListenerThreshold)
                {
                    if (FindLinear(callback) >= 0) return false;

                    AddRaw(hashCode, callback, target);

                    if (_count > SmallListenerThreshold)
                    {
                        RebuildIndex();
                    }

                    return true;
                }

                var indices = _indices!;

                if (!indices.TryGetValue(hashCode, out var index))
                {
                    indices.Add(hashCode, AddRaw(hashCode, callback, target));
                    return true;
                }

                if (index != CollisionBucket)
                {
                    if (DelegateEquals(_callbacks[index]!, callback)) return false;

                    var newIndex = AddRaw(hashCode, callback, target);
                    _collisions ??= new Dictionary<int, List<int>>();
                    _collisions[hashCode] = GetCollisionsListFromPool(index, newIndex);
                    indices[hashCode] = CollisionBucket;
                    return true;
                }

                var bucket = _collisions![hashCode];

                for (var i = 0; i < bucket.Count; i++)
                {
                    if (DelegateEquals(_callbacks[bucket[i]]!, callback)) return false;
                }

                bucket.Add(AddRaw(hashCode, callback, target));
                return true;
            }

            public bool Remove(T callback)
            {
                if (callback == null || _count <= 0) return false;

                var hashCode = callback.GetHashCode();
                return Remove(hashCode, callback);
            }

            public bool Remove(SapTarget<T> target)
            {
                if (target == null || target.Callback == null || _count <= 0) return false;
                return Remove(target.HashCode, target.Callback);
            }

            private bool Remove(int hashCode, T callback)
            {
                if (_count <= SmallListenerThreshold)
                {
                    var index = FindLinear(callback);
                    if (index >= 0)
                    {
                        RemoveAtSwapBackRaw(index);
                        ClearIndex();
                        return true;
                    }

                    return false;
                }

                var indices = _indices!;

                if (!indices.TryGetValue(hashCode, out var mappedIndex)) return false;

                var removeIndex = -1;

                if (mappedIndex != CollisionBucket)
                {
                    if (!DelegateEquals(_callbacks[mappedIndex]!, callback)) return false;

                    removeIndex = mappedIndex;
                    indices.Remove(hashCode);
                }
                else
                {
                    var bucket = _collisions![hashCode];

                    for (var i = 0; i < bucket.Count; i++)
                    {
                        var candidate = bucket[i];
                        if (!DelegateEquals(_callbacks[candidate]!, callback)) continue;

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

                    if (removeIndex < 0) return false;
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

                return true;
            }

            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            private int FindLinear(T callback)
            {
                for (var i = 0; i < _count; i++)
                {
                    if (DelegateEquals(_callbacks[i]!, callback)) return i;
                }

                return -1;
            }

            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            private int AddRaw(int hashCode, T callback, SapTarget<T> target)
            {
                EnsureCapacity();

                var index = _count;
                _hashes[index] = hashCode;
                _callbacks[index] = callback;
                _targets[index] = target;
                _count++;
                return index;
            }

            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            private void RemoveAtSwapBackRaw(int index)
            {
                var lastIndex = _count - 1;

                if (index != lastIndex)
                {
                    _hashes[index] = _hashes[lastIndex];
                    _callbacks[index] = _callbacks[lastIndex];
                    _targets[index] = _targets[lastIndex];
                }

                _hashes[lastIndex] = 0;
                _callbacks[lastIndex] = null;
                _targets[lastIndex] = null;
                _count = lastIndex;
            }

            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            private void EnsureCapacity()
            {
                if (_count < _capacity) return;

                _capacity *= 2;
                Array.Resize(ref _hashes, _capacity);
                Array.Resize(ref _callbacks, _capacity);
                Array.Resize(ref _targets, _capacity);
            }

            private void RebuildIndex()
            {
                if (_indices == null)
                {
                    _indices = new Dictionary<int, int>(_capacity);
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

            [MethodImpl(MethodImplOptions.AggressiveInlining)]
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
                _indices?.Clear();

                if (_collisions == null) return;

                foreach (var collisions in _collisions.Values)
                {
                    ReturnCollisionsListToPool(collisions);
                }

                _collisions.Clear();
            }

            [MethodImpl(MethodImplOptions.AggressiveInlining)]
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
}
#endif
