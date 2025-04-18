using System.Diagnostics;
using CsCheck;
using SpacetimeDB;
using Xunit;

public class MultiDictionaryTests
{
    /// <summary>
    /// Generate a list of KeyValuePairs.
    /// If any two items of the list have duplicate Keys, they are guaranteed to have duplicate Values.
    /// </summary>
    /// <typeparam name="TKey"></typeparam>
    /// <typeparam name="TValue"></typeparam>
    /// <param name="g1"></param>
    /// <param name="g2"></param>
    /// <param name="equality"></param>
    /// <returns></returns>
    Gen<List<KeyValuePair<TKey, TValue>>> ListWithNormalizedDuplicates<TKey, TValue>(Gen<TKey> g1, Gen<TValue> g2, IEqualityComparer<TKey> equality, int ListMinLength = 0, int ListMaxLength = 32)
    where TKey : notnull
    {
        return Gen.Select(g1, g2, (b1, b2) => new KeyValuePair<TKey, TValue>(b1, b2)).List[ListMinLength, ListMaxLength].Select(list =>
            NormalizeDuplicates(list, equality)
        );
    }

    /// <summary>
    /// Normalize a list so that, if any two items have duplicate Keys, they have the same Value.
    /// </summary>
    /// <typeparam name="TKey"></typeparam>
    /// <typeparam name="TValue"></typeparam>
    /// <param name="list"></param>
    /// <param name="equality"></param>
    /// <returns></returns>
    List<KeyValuePair<TKey, TValue>> NormalizeDuplicates<TKey, TValue>(List<KeyValuePair<TKey, TValue>> list, IEqualityComparer<TKey> equality)
    where TKey : notnull
    {
        Dictionary<TKey, TValue> seenKeys = new(equality);
        for (var i = 0; i < list.Count; i++)
        {
            var (b1, b2) = list[i];
            if (seenKeys.ContainsKey(b1))
            {
                list[i] = new(b1, seenKeys[b1]);
            }
            else
            {
                seenKeys[b1] = b2;
            }
        }
        return list;
    }

    [Fact]
    public void Equality()
    {
        // No matter the order we add elements to the multidictionary, the result should be the same.
        ListWithNormalizedDuplicates(Gen.Byte[1, 10], Gen.Byte[1, 10], EqualityComparer<byte>.Default).Sample(list =>
        {
            var m1 = MultiDictionary<byte, byte>.FromEnumerable(list, EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
            Gen.Shuffle(list);
            var m2 = MultiDictionary<byte, byte>.FromEnumerable(list, EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);

            Assert.Equal(m1, m2);
        });

        ListWithNormalizedDuplicates(Gen.Byte[1, 10].Array[1, 10], Gen.Byte[1, 10], SpacetimeDB.Internal.ByteArrayComparer.Instance).Sample(list =>
        {
            var m1 = MultiDictionary<byte[], byte>.FromEnumerable(list, SpacetimeDB.Internal.ByteArrayComparer.Instance, EqualityComparer<byte>.Default);
            Gen.Shuffle(list);
            var m2 = MultiDictionary<byte[], byte>.FromEnumerable(list, SpacetimeDB.Internal.ByteArrayComparer.Instance, EqualityComparer<byte>.Default);

            Assert.Equal(m1, m2);
        });

    }

    /// <summary>
    /// Generate a list of KeyValuePairs, and a list of bools that say whether or not to remove that key-value pair.
    /// </summary>
    /// <typeparam name="TKey"></typeparam>
    /// <typeparam name="TValue"></typeparam>
    /// <param name="g1"></param>
    /// <param name="g2"></param>
    /// <param name="equality"></param>
    /// <param name="maxLength"></param>
    /// <returns></returns>
    Gen<(List<KeyValuePair<TKey, TValue>>, List<bool>)> ListWithRemovals<TKey, TValue>(Gen<TKey> g1, Gen<TValue> g2, IEqualityComparer<TKey> equality,
    int maxLength = 32)
    where TKey : notnull
        => Gen.SelectMany(
            Gen.Int[0, maxLength], (listLength) => Gen.Select(
                // the data itself
                ListWithNormalizedDuplicates(g1, g2, equality, listLength, listLength),
                // whether this element should be added or removed
                Gen.Bool.List[listLength]
            ));

    [Fact]
    public void Removals()
    {
        ListWithRemovals(Gen.Byte[1, 10], Gen.Byte[1, 10], EqualityComparer<byte>.Default).Sample((list, removals) =>
        {
            // Build up two MultiDictionaries:
            // - for m1, add everything, then remove stuff.
            // - for m2, only add the non-removed stuff.
            // The result should be the same.
            var m1 = MultiDictionary<byte, byte>.FromEnumerable(list, EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
            var m2 = new MultiDictionary<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
            foreach (var (kvp, remove) in list.Zip(removals))
            {
                if (remove)
                {
                    m1.Remove(kvp.Key, out var _);
                }
                else
                {
                    m2.Add(kvp.Key, kvp.Value);
                }
            }

            Assert.Equal(m1, m2);
        });
    }

    [Fact]
    public void ShuffleDelta()
    {
        ListWithRemovals(Gen.Byte[1, 10], Gen.Byte[1, 10], EqualityComparer<byte>.Default).Sample((list, removals) =>
        {
            // Check that no matter the order you apply Adds and Removes to a MultiDictionaryDelta, the result is the same.
            var m1 = new MultiDictionaryDelta<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
            var m2 = new MultiDictionaryDelta<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
            var listRemovals = list.Zip(removals).ToList();
            foreach (var (kvp, remove) in listRemovals)
            {
                if (remove)
                {
                    m1.Remove(kvp.Key, kvp.Value);
                }
                else
                {
                    m1.Add(kvp.Key, kvp.Value);
                }
            }
            Gen.Shuffle(listRemovals);
            foreach (var (kvp, remove) in listRemovals)
            {
                if (remove)
                {
                    m2.Remove(kvp.Key, kvp.Value);
                }
                else
                {
                    m2.Add(kvp.Key, kvp.Value);
                }
            }

            Assert.Equal(m1, m2);
        });
    }

    [Fact]
    public void ChunkedRemovals()
    {
        var maxLength = 32;
        Gen.Select(ListWithRemovals(Gen.Byte[1, 10], Gen.Byte[1, 10], EqualityComparer<byte>.Default, maxLength), Gen.Int[0, 32].List[0, 5]).Sample((listRemovals, cuts) =>
        {
            // Test that building up a MultiDictionary an operation-at-a-time is the same as randomly grouping the operations and applying them in chunks using MultiDictionaryDeltas.
            // Pre-normalizes the list of operations so that the same key is always associated with the same value.

            // Note: When looking at test failures for this test, keep in mind we do some post-processing of the sample input data.
            // So the listed operations may be changed slightly while the test is executing.
            // CsCheck doesn't give us a good way to log this :/
            var (list, removals) = listRemovals;
            cuts.Add(0);
            cuts.Add(maxLength);
            cuts = cuts.Select(cut => int.Min(cut, list.Count)).ToList();
            cuts.Sort();

            var viaAddRemove = new MultiDictionary<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
            var viaChunkDeltas = new MultiDictionary<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);

            if (list.Count == 0)
            {
                return;
            }

            foreach (var (start, end) in cuts.Zip(cuts.Skip(1)))
            {
                var delta = new MultiDictionaryDelta<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);

                foreach (var (item, remove) in list[start..end].Zip(removals[start..end]))
                {
                    // it's an error to remove-too-many-times with the Delta api.
                    // so, don't remove anything we don't have.
                    var remove_ = remove && viaAddRemove.Contains(item);
                    if (remove_)
                    {
                        viaAddRemove.Remove(item.Key, out var _);
                        delta.Remove(item.Key, item.Value);
                    }
                    else
                    {
                        viaAddRemove.Add(item.Key, item.Value);
                        delta.Add(item.Key, item.Value);
                    }
                }
                foreach (var (key, value) in viaChunkDeltas.WillRemove(delta))
                {
                    Assert.True(viaChunkDeltas.Contains(new(key, value)));
                }
                var wasInserted = new List<KeyValuePair<byte, byte>>();
                var wasMaybeUpdated = new List<(byte key, byte oldValue, byte newValue)>();
                var wasRemoved = new List<KeyValuePair<byte, byte>>();

                viaChunkDeltas.Apply(delta, wasInserted, wasMaybeUpdated, wasRemoved);
                foreach (var (key, value) in wasInserted)
                {
                    Assert.True(viaChunkDeltas.Contains(new(key, value)));
                }
                foreach (var (key, oldValue, newValue) in wasMaybeUpdated)
                {
                    Assert.True(viaChunkDeltas.Contains(new(key, newValue)) && oldValue == newValue);
                }
                foreach (var (key, value) in wasRemoved)
                {
                    Assert.False(viaChunkDeltas.Contains(new(key, value)));
                }
                Assert.Equal(viaAddRemove, viaChunkDeltas);
            }
        }, iter: 10_000);
    }

    [Fact]
    public void ChunkedRemovalsWithValueChanges()
    {
        // Test that building up a MultiDictionary an operation-at-a-time is the same as randomly grouping the operations and applying them in chunks using MultiDictionaryDeltas.
        // Unlike ChunkedRemovals, this doesn't pre-normalize the list of operations so that the same key always has the same value.
        // Instead, in a particular chunk of operations, if a new value is assigned to a key, we go in and remove the old value the correct number of times.

        var maxLength = 32;
        // Don't pre-normalize the list.
        // We want to normalize per-chunk instead.
        var listRemovalsNonNormalized = Gen.SelectMany(
            Gen.Int[0, maxLength], (listLength) => Gen.Select(
                // the data itself
                Gen.Select(Gen.Byte[1, 10], Gen.Byte[1, 10], (b1, b2) => new KeyValuePair<byte, byte>(b1, b2)).List[listLength, listLength],
                // whether this element should be added or removed
                Gen.Bool.List[listLength]
            ));

        Gen.Select(listRemovalsNonNormalized, Gen.Int[0, maxLength].List[0, 5]).Select((listRemovals, cuts) =>
        {
            var (list, removals) = listRemovals;

            cuts.Add(0);
            cuts.Add(maxLength);
            cuts = cuts.Select(cut => int.Min(cut, list.Count)).ToList();
            cuts.Sort();

            var listWithRemovals = list.Zip(removals).ToList();

            var equalityComparer = EqualityComparer<byte>.Default;

            var viaAddRemove = new MultiDictionary<byte, byte>(equalityComparer, equalityComparer);

            return cuts.Zip(cuts.Skip(1)).Select((range) =>
            {
                var (start, end) = range;
                var listChunk = list[start..end];
                NormalizeDuplicates(listChunk, equalityComparer);
                var removalsChunk = removals[start..end];

                return (listChunk, removalsChunk);
            }).ToList();
        })
        .Sample((chunkedListRemovals) =>
        {
            var equalityComparer = EqualityComparer<byte>.Default;

            var viaAddRemove = new MultiDictionary<byte, byte>(equalityComparer, equalityComparer);
            var viaChunkDeltas = new MultiDictionary<byte, byte>(equalityComparer, equalityComparer);

            foreach (var (listChunk, removalsChunk) in chunkedListRemovals)
            {
                var delta = new MultiDictionaryDelta<byte, byte>(equalityComparer, equalityComparer);

                foreach (var (item, remove) in listChunk.Zip(removalsChunk))
                {
                    if (viaAddRemove.Multiplicity(item.Key) > 0 && !equalityComparer.Equals(viaAddRemove[item.Key], item.Value))
                    {
                        var oldValue = viaAddRemove[item.Key];
                        // This chunk is going to change the value associated with this key.
                        // Remove it the correct number of times.
                        var mult = viaAddRemove.Multiplicity(item.Key);
                        for (uint i = 0; i < mult; i++)
                        {
                            viaAddRemove.Remove(item.Key, out var _);
                            delta.Remove(item.Key, oldValue);
                        }
                    }

                    // it's an error to remove-too-many-times with the Delta api.
                    // so, don't remove anything we don't have.
                    var remove_ = remove && viaAddRemove.Contains(item);
                    if (remove_)
                    {
                        viaAddRemove.Remove(item.Key, out var _);
                        delta.Remove(item.Key, item.Value);
                    }
                    else
                    {
                        viaAddRemove.Add(item.Key, item.Value);
                        delta.Add(item.Key, item.Value);
                    }
                }
                foreach (var (key, value) in viaChunkDeltas.WillRemove(delta))
                {
                    Assert.True(viaChunkDeltas.Contains(new(key, value)));
                }
                var wasInserted = new List<KeyValuePair<byte, byte>>();
                var wasMaybeUpdated = new List<(byte key, byte oldValue, byte newValue)>();
                var wasRemoved = new List<KeyValuePair<byte, byte>>();

                viaChunkDeltas.Apply(delta, wasInserted, wasMaybeUpdated, wasRemoved);
                foreach (var (key, value) in wasInserted)
                {
                    Assert.True(viaChunkDeltas.Contains(new(key, value)));
                }
                foreach (var (key, oldValue, newValue) in wasMaybeUpdated)
                {
                    Assert.True(viaChunkDeltas.Contains(new(key, newValue)));
                }
                foreach (var (key, value) in wasRemoved)
                {
                    Assert.False(viaChunkDeltas.Contains(new(key, value)));
                }
                Assert.Equal(viaAddRemove, viaChunkDeltas);
            }
        }, iter: 10_000);
    }

    [Fact]
    public void IdentitiesWorkAsPrimaryKeys()
    {
        // GenericEqualityComparer used to have a bug, this is a regression test for that.
        var identity = Identity.From(Convert.FromBase64String("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY="));
        var hashSet = new HashSet<object>(GenericEqualityComparer.Instance)
        {
            identity
        };
        Debug.Assert(hashSet.Contains(identity));

        var dict = new MultiDictionary<object, byte>(GenericEqualityComparer.Instance, EqualityComparer<byte>.Default);

        dict.Add(identity, 3);
        dict.Add(identity, 3);

        var delta = new MultiDictionaryDelta<object, byte>(GenericEqualityComparer.Instance, EqualityComparer<byte>.Default);
        delta.Remove(identity, 3);
        delta.Remove(identity, 3);
        var wasInserted = new List<KeyValuePair<object, byte>>();
        var wasMaybeUpdated = new List<(object key, byte oldValue, byte newValue)>();
        var wasRemoved = new List<KeyValuePair<object, byte>>();
        dict.Apply(delta, wasInserted, wasMaybeUpdated, wasRemoved);
    }

    [Fact]
    public void InsertThenDeleteOfOldRow()
    {
        var dict = new MultiDictionary<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
        dict.Add(1, 2);

        var delta = new MultiDictionaryDelta<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
        delta.Add(1, 2);
        delta.Add(1, 3);
        delta.Remove(1, 2);
        delta.Remove(1, 2);

        var wasInserted = new List<KeyValuePair<byte, byte>>();
        var wasMaybeUpdated = new List<(byte key, byte oldValue, byte newValue)>();
        var wasRemoved = new List<KeyValuePair<byte, byte>>();
        dict.Apply(delta, wasInserted, wasMaybeUpdated, wasRemoved);
#pragma warning disable xUnit2017
        Assert.True(wasMaybeUpdated.Contains((1, 2, 3)), $"{dict}: {wasMaybeUpdated}");
#pragma warning restore xUnit2017

        // And one more permutation.

        dict = new MultiDictionary<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
        dict.Add(1, 2);

        delta = new MultiDictionaryDelta<byte, byte>(EqualityComparer<byte>.Default, EqualityComparer<byte>.Default);
        delta.Add(1, 3);
        delta.Remove(1, 2);
        delta.Remove(1, 2);
        delta.Add(1, 2);

        wasInserted = new List<KeyValuePair<byte, byte>>();
        wasMaybeUpdated = new List<(byte key, byte oldValue, byte newValue)>();
        wasRemoved = new List<KeyValuePair<byte, byte>>();
        dict.Apply(delta, wasInserted, wasMaybeUpdated, wasRemoved);
#pragma warning disable xUnit2017
        Assert.True(wasMaybeUpdated.Contains((1, 2, 3)), $"{dict}: {wasMaybeUpdated}");
#pragma warning restore xUnit2017
    }
}