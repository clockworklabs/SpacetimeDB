using System.Collections;
using System.Collections.Generic;
using System.Linq.Expressions;
using System.Runtime.CompilerServices;

#nullable enable

/// <summary>
/// A hashset optimized to store small numbers of values of type T.
/// Used because many of the hash sets in our BTreeIndexes store only one value.
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
            return new SingleElementEnumerator<T>(Value.Value);
        }
        else if (Values != null)
        {
            return Values.GetEnumerator();
        }
        else
        {
            return new NoElementEnumerator<T>();
        }
    }

    IEnumerator IEnumerable.GetEnumerator()
    {
        return GetEnumerator();
    }
}

/// <summary>
/// This is a silly object.
/// </summary>
/// <typeparam name="T"></typeparam>
internal struct SingleElementEnumerator<T> : IEnumerator<T>
where T : struct
{
    T value;
    enum State
    {
        Unstarted,
        Started,
        finished
    }

    State state;

    public SingleElementEnumerator(T value)
    {
        this.value = value;
        state = State.Unstarted;
    }

    public T Current => value;

    object IEnumerator.Current => Current;

    public void Dispose()
    {
    }

    public bool MoveNext()
    {
        if (state == State.Unstarted)
        {
            state = State.Started;
            return true;
        }
        else if (state == State.Started)
        {
            state = State.finished;
            return false;
        }
        return false;
    }

    public void Reset()
    {
        state = State.Started;
    }
}

/// <summary>
/// This is a very silly object.
/// </summary>
/// <typeparam name="T"></typeparam>
internal struct NoElementEnumerator<T> : IEnumerator<T>
where T : struct
{
    public T Current => new();

    object IEnumerator.Current => Current;

    public void Dispose()
    {
    }

    public bool MoveNext()
    {
        return false;
    }

    public void Reset()
    {
    }
}

#nullable disable