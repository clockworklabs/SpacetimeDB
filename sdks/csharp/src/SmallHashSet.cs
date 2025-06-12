using System.Collections;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Threading;

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
    static readonly EQ DefaultEqualityComparer = new();
    static readonly ThreadLocal<Stack<HashSet<T>>> Pool = new(() => new(), false);

    // Assuming each HashSet<T> uses 128 bytes of memory, this means
    // our pool will use at most 512 MB of memory per thread.
    // Since in the current design of the SDK, only the main thread produces SmallHashSets,
    // this should be fine.
    static readonly int MAX_POOL_SIZE = 4_000_000;

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
                Values = AllocHashSet();
                Values.Add(newValue);
                Values.Add(Value.Value);
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
        else if (Values != null && Values.Contains(remValue))
        {
            Values.Remove(remValue);

            // If we're not storing values and there's room in the pool, reuse this allocation.
            // Otherwise, we can keep the allocation around: all of the logic in this class will still
            // work.
            var LocalPool = Pool.Value;
            if (Values.Count == 0 && LocalPool.Count < MAX_POOL_SIZE)
            {
                LocalPool.Push(Values);
                Values = null;
            }
        }
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public bool Contains(T value)
    {
        if (Values != null)
        {
            return Values.Contains(value);
        }
        else if (Value != null)
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

    /// <summary>
    /// Allocate a new HashSet with the capacity to store at least 2 elements.
    /// </summary>
    /// <returns></returns>
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private static HashSet<T> AllocHashSet()
    {
        if (Pool.Value.TryPop(out var result))
        {
            return result;
        }
        else
        {
            return new(2, DefaultEqualityComparer);
        }
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


internal struct SmallHashSetOfPreHashedRow : IEnumerable<PreHashedRow>
{
    static readonly ThreadLocal<Stack<HashSet<PreHashedRow>>> Pool = new(() => new(), false);

    // Assuming each HashSet<T> uses 128 bytes of memory, this means
    // our pool will use at most 512 MB of memory per thread.
    // Since in the current design of the SDK, only the main thread produces SmallHashSets,
    // this should be fine.
    static readonly int MAX_POOL_SIZE = 4_000_000;

    // Invariant: zero or one of the following is not null.
    PreHashedRow? Value;
    HashSet<PreHashedRow>? Values;

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public void Add(PreHashedRow newValue)
    {
        if (Values == null)
        {
            if (Value == null)
            {
                Value = newValue;
            }
            else
            {
                Values = AllocHashSet();
                Values.Add(newValue);
                Values.Add(Value.Value);
                Value = null;
            }
        }
        else
        {
            Values.Add(newValue);
        }
    }

    public void Remove(PreHashedRow remValue)
    {
        if (Value != null && Value.Value.Equals(remValue))
        {
            Value = null;
        }
        else if (Values != null && Values.Contains(remValue))
        {
            Values.Remove(remValue);

            // If we're not storing values and there's room in the pool, reuse this allocation.
            // Otherwise, we can keep the allocation around: all of the logic in this class will still
            // work.
            var LocalPool = Pool.Value;
            if (Values.Count == 0 && LocalPool.Count < MAX_POOL_SIZE)
            {
                LocalPool.Push(Values);
                Values = null;
            }
        }
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public bool Contains(PreHashedRow value)
    {
        if (Values != null)
        {
            return Values.Contains(value);
        }
        else if (Value != null)
        {
            return Value.Value.Equals(value);
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

    public IEnumerator<PreHashedRow> GetEnumerator()
    {
        if (Value != null)
        {
            return new SingleElementEnumerator<PreHashedRow>(Value.Value);
        }
        else if (Values != null)
        {
            return Values.GetEnumerator();
        }
        else
        {
            return new NoElementEnumerator<PreHashedRow>();
        }
    }

    IEnumerator IEnumerable.GetEnumerator()
    {
        return GetEnumerator();
    }

    /// <summary>
    /// Allocate a new HashSet with the capacity to store at least 2 elements.
    /// </summary>
    /// <returns></returns>
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private static HashSet<PreHashedRow> AllocHashSet()
    {
        if (Pool.Value.TryPop(out var result))
        {
            return result;
        }
        else
        {
            return new(2, PreHashedRowComparer.Default);
        }
    }
}


#nullable disable