// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.

namespace SpacetimeDB;

using System.Numerics;
using System.Runtime.InteropServices;

/// <summary>Represents a 128-bit signed integer.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct I128 : IEquatable<I128>, IComparable, IComparable<I128>
{
#if BIGENDIAN
    private readonly ulong _upper;
    private readonly ulong _lower;
#else
    private readonly ulong _lower;
    private readonly ulong _upper;
#endif

    /// <summary>Initializes a new instance of the <see cref="I128" /> struct.</summary>
    /// <param name="upper">The upper 64-bits of the 128-bit value.</param>
    /// <param name="lower">The lower 64-bits of the 128-bit value.</param>
    public I128(ulong upper, ulong lower)
    {
        _upper = upper;
        _lower = lower;
    }

    internal ulong Lower => _lower;

    internal ulong Upper => _upper;

    /// <inheritdoc cref="IComparable.CompareTo(object)" />
    public int CompareTo(object? value)
    {
        if (value is I128 other)
        {
            return CompareTo(other);
        }
        else if (value is null)
        {
            return 1;
        }
        else
        {
            throw new ArgumentException();
        }
    }

    /// <inheritdoc cref="IComparable{T}.CompareTo(T)" />
    public int CompareTo(I128 value)
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
    public static bool operator <(I128 left, I128 right)
    {
        if (IsNegative(left) == IsNegative(right))
        {
            return (left._upper < right._upper)
                || ((left._upper == right._upper) && (left._lower < right._lower));
        }
        else
        {
            return IsNegative(left);
        }
    }

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_GreaterThan(TSelf, TOther)" />
    public static bool operator >(I128 left, I128 right)
    {
        if (IsNegative(left) == IsNegative(right))
        {
            return (left._upper > right._upper)
                || ((left._upper == right._upper) && (left._lower > right._lower));
        }
        else
        {
            return IsNegative(right);
        }
    }

    /// <inheritdoc cref="INumberBase{TSelf}.IsNegative(TSelf)" />
    public static bool IsNegative(I128 value) => (long)value._upper < 0;

    private BigInteger AsBigInt() =>
        new(
            MemoryMarshal.AsBytes(stackalloc[] { this }),
            isUnsigned: false,
            isBigEndian: !BitConverter.IsLittleEndian
        );

    /// <inheritdoc cref="object.ToString()" />
    public override string ToString() => AsBigInt().ToString();

    /// <summary>Implicitly converts a <see cref="int" /> value to a 128-bit signed integer.</summary>
    /// <param name="value">The value to convert.</param>
    /// <returns><paramref name="value" /> converted to a 128-bit signed integer.</returns>
    public static implicit operator I128(int value)
    {
        long lower = value;
        return new I128((ulong)(lower >> 63), (ulong)lower);
    }
}
