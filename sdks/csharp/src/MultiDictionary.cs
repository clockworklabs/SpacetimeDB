using System;
using System.Linq;
using System.Text;
using System.Collections.Generic;
using System.Diagnostics;
using System.Data;

namespace SpacetimeDB
{
    /// <summary>
    /// A dictionary that may have multiple copies of a key-value pair.
    /// Note that a particular key only maps to one value -- it is a logical error
    /// to insert the same key with different values.
    /// </summary>
    /// <typeparam name="TKey"></typeparam>
    /// <typeparam name="TValue"></typeparam>
    internal struct MultiDictionary<TKey, TValue> : IEquatable<MultiDictionary<TKey, TValue>>
    {
        // The actual data.
        readonly Dictionary<TKey, (TValue Value, uint Multiplicity)> RawDict;
        readonly IEqualityComparer<TValue> ValueComparer;

        /// <summary>
        /// Construct a MultiDictionary.
        /// 
        /// This is the only valid constructor for a Multidictionary - using the parameterless constructor
        /// will result in null pointer errors. But we can't enforce this because of Unity.
        /// </summary>
        /// <param name="keyComparer"></param>
        public MultiDictionary(IEqualityComparer<TKey> keyComparer, IEqualityComparer<TValue> valueComparer)
        {
            RawDict = new(keyComparer);
            ValueComparer = valueComparer;
        }

        public static MultiDictionary<TKey, TValue> FromEnumerable(IEnumerable<KeyValuePair<TKey, TValue>> enumerable, IEqualityComparer<TKey> keyComparer, IEqualityComparer<TValue> valueComparer)
        {
            var result = new MultiDictionary<TKey, TValue>(keyComparer, valueComparer);
            foreach (var item in enumerable)
            {
                result.Add(item.Key, item.Value);
            }
            return result;
        }

        /// <summary>
        /// Return the count WITHOUT multiplicities.
        /// This is mathematically unnatural, but cheap.
        /// </summary>
        public readonly uint CountDistinct => (uint)RawDict.Count;

        /// <summary>
        /// Return the count WITH multiplicities.
        /// </summary>
        public readonly uint Count => RawDict.Select(item => item.Value.Multiplicity).Aggregate(0u, (a, b) => a + b);

        /// <summary>
        /// Add a key-value-pair to the multidictionary.
        /// If the key is already present, its associated value must satisfy
        /// keyComparer.Equals(value, item.Value).
        /// </summary>
        /// <param name="item"></param>
        /// <returns>Whether the key is entirely new to the dictionary. If it was already present, we assert that the old value is equal to the new value.</returns>
        public bool Add(TKey key, TValue value)
        {
            if (value == null)
            {
                throw new NullReferenceException("Null values are forbidden in multidictionary");
            }
            Debug.Assert(RawDict != null);
            Debug.Assert(key != null);
            if (RawDict.TryGetValue(key, out var result))
            {
                Debug.Assert(ValueComparer.Equals(value, result.Value), "Added key-value pair with mismatched value to existing data");
                RawDict[key] = (value, result.Multiplicity + 1);
                return false;
            }
            else
            {
                RawDict[key] = (value, 1);
                return true;
            }
        }

        /// <summary>
        /// Completely clear the multidictionary.
        /// </summary>
        public void Clear()
        {
            RawDict.Clear();
        }

        /// <summary>
        /// Whether the multidictionary contains any copies of an item.
        /// </summary>
        /// <param name="item"></param>
        /// <returns></returns>
        public bool Contains(KeyValuePair<TKey, TValue> item)
        {
            if (RawDict.TryGetValue(item.Key, out var result))
            {
                return ValueComparer.Equals(item.Value, result.Value);
            }
            return false;
        }

        /// <summary>
        /// Remove a key from the dictionary.
        /// </summary>
        /// <param name="key"></param>
        /// <returns>Whether the last copy of the key was removed.</returns>
        public bool Remove(TKey key, out TValue row)
        {
            if (RawDict.TryGetValue(key, out var result))
            {
                row = result.Value;
                if (result.Multiplicity == 1)
                {
                    RawDict.Remove(key);
                    return true;
                }
                else
                {
                    RawDict[key] = (result.Value, result.Multiplicity - 1);
                    return false;
                }
            }
            row = default!; // uhh, this might be null. Good thing it's an internal method?
            return false;
        }

        public bool Equals(MultiDictionary<TKey, TValue> other)
        {
            foreach (var item in RawDict)
            {
                var (key, (value, multiplicity)) = item;
                if (other.RawDict.TryGetValue(key, out var otherVM))
                {
                    var (otherValue, otherMultiplicity) = otherVM;
                    if (!(ValueComparer.Equals(value, otherValue) && multiplicity == otherMultiplicity))
                    {
                        return false;
                    }
                }
            }

            return true;
        }

        public readonly IEnumerable<TValue> Values
        {
            get
            {

                return RawDict.Select(item => item.Value.Value);
            }
        }

        public readonly IEnumerable<KeyValuePair<TKey, TValue>> Entries
        {
            get
            {
                return RawDict.Select(item => new KeyValuePair<TKey, TValue>(item.Key, item.Value.Value));
            }
        }

        /// <summary>
        /// Iterate the rows that will be removed when `delta` is applied.
        /// </summary>
        /// <param name="delta"></param>
        /// <returns></returns>
        public readonly IEnumerable<KeyValuePair<TKey, TValue>> WillRemove(MultiDictionaryDelta<TKey, TValue> delta)
        {
            var self = this;
            return delta.Entries.Where(entry =>
            {
                var entryDelta = (int)entry.Value.Inserts - (int)entry.Value.Removes;
                if (entryDelta >= 0)
                {
                    return false;
                }
                if (self.RawDict.TryGetValue(entry.Key, out var mine))
                {
                    var resultMultiplicity = (int)mine.Multiplicity + entryDelta;
                    return resultMultiplicity <= 0;
                }
                else
                {
                    Log.Warn($"Want to remove row with key {entry.Key}, but it doesn't exist!");
                    return false;
                }
            }).Select(entry => new KeyValuePair<TKey, TValue>(entry.Key, entry.Value.Value));
        }

        /// <summary>
        /// Apply a collection of changes to a multidictionary.
        /// </summary>
        /// <param name="delta">The changes to apply.</param>
        /// <param name="onInsert">Called on rows that were inserted.</param>
        /// <param name="onUpdate">Called on rows that were updated (not including multiplicity changes).</param>
        /// <param name="onRemove">Called on rows that were removed.</param>
        public void Apply(MultiDictionaryDelta<TKey, TValue> delta, List<KeyValuePair<TKey, TValue>> wasInserted, List<(TKey Key, TValue OldValue, TValue NewValue)> wasUpdated, List<KeyValuePair<TKey, TValue>> wasRemoved)
        {
            foreach (var (key, their) in delta.Entries)
            {
                var entryDelta = (int)their.Inserts - (int)their.Removes;

                if (RawDict.TryGetValue(key, out var my))
                {
                    var newMultiplicity = (int)my.Multiplicity + entryDelta;
                    if (newMultiplicity > 0)
                    {
                        if (ValueComparer.Equals(my.Value, their.Value))
                        {
                            // Update the count, NOT dispatching an update event.

                            // It sort of matters if we use my.Value or their.Value here:
                            // we'd prefer to keep stricter equalities like pointer equality intact if possible.
                            // So even though my.Value and theirValue are "equal", prefer using my.Value for
                            // pointer stability reasons.
                            RawDict[key] = (my.Value, (uint)newMultiplicity);
                        }
                        else
                        {
                            // Update the count and value, dispatching an update event.
                            Debug.Assert(their.Removes >= my.Multiplicity, "Row was not removed enough times in update.");

                            // Here, we actually have meaningful changes, so use their value.
                            RawDict[key] = (their.Value, (uint)newMultiplicity);
                            wasUpdated.Add((key, my.Value, their.Value)); // store both the old and new values.
                        }
                    }
                    else // if (newMultiplicity <= 0)
                    {
                        // This is a removal.
                        if (newMultiplicity < 0)
                        {
                            PseudoThrow($"Internal error: Removing row with key {key} {-entryDelta} times, but it is only present {my.Multiplicity} times.");
                        }

                        RawDict.Remove(key);
                        wasRemoved.Add(new(key, their.Value));
                    }
                }
                else
                {
                    // Key is not present in map.
                    if (entryDelta < 0)
                    {
                        PseudoThrow($"Internal error: Removing row with key {key} {-entryDelta} times, but it not present.");
                    }
                    else if (entryDelta == 0)
                    {
                        // Hmm.
                        // This is not actually a problem.
                        // Do nothing.
                    }
                    else if (entryDelta > 0)
                    {
                        RawDict[key] = (their.Value, (uint)entryDelta);
                        wasInserted.Add(new(key, their.Value));
                    }
                }
            }


        }

        /// <summary>
        /// Raise a debug assertion failure in debug mode, otherwise just warn and keep going.
        /// </summary>
        /// <param name="message"></param>
        private void PseudoThrow(string message)
        {
            Log.Warn(message);
            Debug.Assert(false, message);
        }

        public override string ToString()
        {
            StringBuilder result = new();
            result.Append("SpacetimeDB.MultiDictionary { ");
            foreach (var item in RawDict)
            {
                result.Append($"({item.Key}: {item.Value.Value}) x {item.Value.Multiplicity}, ");
            }
            result.Append("}");
            return result.ToString();
        }

    }

    /// <summary>
    /// A bulk change to a multidictionary. Allows both adding and removing rows.
    /// 
    /// Can be applied to a multidictionary, and also inspected before application to see
    /// what rows will be deleted. (This is used for OnBeforeDelete.)
    /// 
    /// Curiously, the order of operations applied to a MultiDictionaryDelta does not matter.
    /// No matter the order of Add and Remove calls on a delta, when the Delta is applied,
    /// the result will be the same, as long as the Add and Remove *counts* for each KeyValuePair are
    /// the same.
    /// (This means that this is a "conflict-free replicated data type", unlike MultiDictionary.)
    /// (MultiDictionary would also be "conflict-free" if it didn't support Remove.)
    ///
    /// The delta may include value updates.
    /// A value can be updated multiple times, but each update must set the result to the same value.
    /// When applying a delta, if the target multidictionary has multiple copies of (key, value) pair,
    /// the row must be removed exactly the correct number of times. It can be inserted an arbitrary number of times.
    ///
    /// When removing a row for an update, it is legal for the passed value to be equal to EITHER the old value or the new value. 
    /// (This is because I'm not sure what SpacetimeDB core does.)
    /// </summary>
    /// <typeparam name="TKey"></typeparam>
    /// <typeparam name="TValue"></typeparam>
    internal struct MultiDictionaryDelta<TKey, TValue> : IEquatable<MultiDictionaryDelta<TKey, TValue>>
    {
        /// <summary>
        /// For each key, track its NEW value (or old value, but only if we have never seen the new value).
        /// Also track the number of times it has been removed and inserted.
        /// We keep these separate so that we can debug-assert that a KVP has been removed enough times (in case
        /// there are multiple copies of the KVP in the map we get applied to.)
        /// </summary>
        readonly Dictionary<TKey, (TValue Value, uint Removes, uint Inserts)> RawDict;

        readonly IEqualityComparer<TValue> ValueComparer;

        /// <summary>
        /// Construct a MultiDictionaryDelta.
        /// 
        /// This is the only valid constructor for a MultiDictionaryDelta - using the parameterless constructor
        /// will result in null pointer errors. But we can't enforce this because of Unity.
        /// </summary>
        /// <param name="keyComparer"></param>

        public MultiDictionaryDelta(IEqualityComparer<TKey> keyComparer, IEqualityComparer<TValue> valueComparer)
        {
            RawDict = new(keyComparer);
            ValueComparer = valueComparer;
        }

        /// <summary>
        /// Add a key-value-pair to the multidictionary.
        /// If the key is already present, its associated value must satisfy
        /// keyComparer.Equals(value, item.Value).
        /// </summary>
        /// <param name="item"></param>
        public void Add(TKey key, TValue value)
        {
            if (value == null)
            {
                throw new NullReferenceException("Null values are forbidden in multidictionary");
            }
            Debug.Assert(RawDict != null);
            Debug.Assert(key != null);
            if (RawDict.TryGetValue(key, out var result))
            {
                if (result.Inserts > 0)
                {
                    Debug.Assert(ValueComparer.Equals(value, result.Value), "Added key-value pair with mismatched value to existing data");
                }
                // Now, make sure we override the value, since it may have been added in a remove, which MAY have passed the
                // out-of-date value.
                RawDict[key] = (value, result.Removes, result.Inserts + 1);
            }
            else
            {
                RawDict[key] = (value, 0, 1);
            }
        }

        /// <summary>
        /// Completely clear the multidictionary.
        /// </summary>
        public void Clear()
        {
            RawDict.Clear();
        }

        /// <summary>
        /// Remove a key from the dictionary.
        /// </summary>
        /// <param name="key"></param>
        public void Remove(TKey key, TValue value)
        {
            if (RawDict.TryGetValue(key, out var result))
            {
                // DON'T assert that result.Value == value: if an update is happening, that may not be the case.
                RawDict[key] = (result.Value, result.Removes + 1, result.Inserts);
            }
            else
            {
                RawDict[key] = (value, 1, 0);
            }
        }

        public override string ToString()
        {
            StringBuilder result = new();
            result.Append("SpacetimeDB.MultiDictionaryDelta { ");
            foreach (var item in RawDict)
            {
                result.Append($"({item.Key}: {item.Value.Value}) x (+{item.Value.Inserts} -{item.Value.Removes}), ");
            }
            result.Append("}");
            return result.ToString();
        }

        public bool Equals(MultiDictionaryDelta<TKey, TValue> other)
        {
            foreach (var item in RawDict)
            {
                var (key, my) = item;
                if (other.RawDict.TryGetValue(key, out var their))
                {
                    if (!(ValueComparer.Equals(my.Value, their.Value) && my.Inserts == their.Inserts && my.Removes == their.Removes))
                    {
                        return false;
                    }
                }
            }

            return true;
        }

        public readonly IEnumerable<KeyValuePair<TKey, (TValue Value, uint Removes, uint Inserts)>> Entries
        {
            get
            {
                return RawDict;
            }
        }
    }
}