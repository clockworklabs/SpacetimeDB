using System.Collections;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

#nullable enable

/// <summary>
/// A hashset optimized to store small numbers of values of type T.
/// Used because many of the hash sets in our BTree indexes
/// </summary>
/// <typeparam name="T"></typeparam>
internal struct SmallHashSet<T, EQ> : IEnumerable<T>
where T : struct
where EQ : IEqualityComparer<T>, new()
{
    static EQ DefaultEqualityComparer = new();

    // Invariant: zero or one of the following is not null.
    T? Value;
    HashSet<T>? Values;

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public void Add(T newValue)
    {
        if (Values == null)
        {
            if (Value == null)
            {
                Value = newValue;
            }
            else
            {
                Values = new(2, DefaultEqualityComparer)
                {
                    newValue,
                    Value.Value
                };
                Value = null;
            }
        }
        else
        {
            Values.Add(newValue);
        }
    }

    public void Remove(T remValue)
    {
        if (Value != null && DefaultEqualityComparer.Equals(Value.Value, remValue))
        {
            Value = null;
        }
        if (Values != null && Values.Contains(remValue))
        {
            Values.Remove(remValue);
            // Do not try to go back to single-row state.
            // We might as well keep the allocation around if this set has needed to store multiple values before.
        }
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public bool Contains(T value)
    {
        if (Values != null)
        {
            return Values.Contains(value);
        }
        if (Value != null)
        {
            return DefaultEqualityComparer.Equals(Value.Value, value);
        }
        return false;
    }

    public int Count
    {
        get
        {
            if (Value != null)
            {
                return 1;
            }
            else if (Values != null)
            {
                return Values.Count;
            }
            return 0;
        }

    }

    public IEnumerator<T> GetEnumerator()
    {
        if (Value != null)
        {
            yield return Value.Value;
        }
        else if (Values != null)
        {
            foreach (var value in Values)
            {
                yield return value;
            }
        }
    }

    IEnumerator IEnumerable.GetEnumerator()
    {
        return GetEnumerator();
    }
}
#nullable disable