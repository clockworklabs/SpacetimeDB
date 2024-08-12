// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.

namespace SpacetimeDB;

using System;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;

/// <summary>Represents a 256-bit signed integer.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct I256 : IEquatable<I256>, IComparable, IComparable<I256>
{
#if BIGENDIAN
    private readonly U128 _upper;
    private readonly U128 _lower;
#else
    private readonly U128 _lower;
    private readonly U128 _upper;
#endif

    /// <summary>Initializes a new instance of the <see cref="I256" /> struct.</summary>
    /// <param name="upper">The upper 128-bits of the 256-bit value.</param>
    /// <param name="lower">The lower 128-bits of the 256-bit value.</param>
    public I256(U128 upper, U128 lower)
    {
        _upper = upper;
        _lower = lower;
    }

    /// <inheritdoc cref="IComparable.CompareTo(object)" />
    public int CompareTo(object? value)
    {
        if (value is I256 other)
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
    public int CompareTo(I256 value)
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
    public static bool operator <(I256 left, I256 right)
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
    public static bool operator >(I256 left, I256 right)
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

    public static bool IsNegative(I256 value) => (long)value._upper.Upper < 0;

    /// <inheritdoc cref="object.ToString()" />
    public override string ToString() => BigIntHelpers.ToString(this, false);
}
