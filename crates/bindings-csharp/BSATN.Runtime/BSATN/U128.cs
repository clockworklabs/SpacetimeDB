// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.

namespace SpacetimeDB;

using System.Numerics;
using System.Runtime.InteropServices;

/// <summary>Represents a 128-bit unsigned integer.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct U128 : IEquatable<U128>, IComparable, IComparable<U128>
{
    internal const int Size = 16;

#if BIGENDIAN
    private readonly ulong _upper;
    private readonly ulong _lower;
#else
    private readonly ulong _lower;
    private readonly ulong _upper;
#endif

    /// <summary>Initializes a new instance of the <see cref="U128" /> struct.</summary>
    /// <param name="upper">The upper 64-bits of the 128-bit value.</param>
    /// <param name="lower">The lower 64-bits of the 128-bit value.</param>
    public U128(ulong upper, ulong lower)
    {
        _upper = upper;
        _lower = lower;
    }

    internal ulong Lower => _lower;

    internal ulong Upper => _upper;

    /// <inheritdoc cref="IComparable.CompareTo(object)" />
    public int CompareTo(object? value)
    {
        if (value is U128 other)
        {
            return CompareTo(other);
        }
        else if (value is null)
        {
            return 1;
        }
        else
        {
            throw new ArgumentException("Argument must be a U128", nameof(value));
        }
    }

    /// <inheritdoc cref="IComparable{T}.CompareTo(T)" />
    public int CompareTo(U128 value)
    {
        if (this < value)
        {
            return -1;
        }
        else if (this > value)
        {
            return 1;
        }
        else
        {
            return 0;
        }
    }

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_LessThan(TSelf, TOther)" />
    public static bool operator <(U128 left, U128 right)
    {
        return (left._upper < right._upper)
            || (left._upper == right._upper) && (left._lower < right._lower);
    }

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_GreaterThan(TSelf, TOther)" />
    public static bool operator >(U128 left, U128 right)
    {
        return (left._upper > right._upper)
            || (left._upper == right._upper) && (left._lower > right._lower);
    }

    private BigInteger AsBigInt() =>
        new(
            MemoryMarshal.AsBytes(stackalloc[] { this }),
            isUnsigned: false,
            isBigEndian: !BitConverter.IsLittleEndian
        );

    /// <inheritdoc cref="object.ToString()" />
    public override string ToString() => AsBigInt().ToString();
}
