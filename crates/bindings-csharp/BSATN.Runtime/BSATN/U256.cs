namespace SpacetimeDB;

using System;
using System.Diagnostics.CodeAnalysis;
using System.Numerics;
using System.Runtime.InteropServices;

/// <summary>Represents a 128-bit unsigned integer.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct U256 : IEquatable<U256>, IComparable, IComparable<U256>
{
#if BIGENDIAN
    private readonly U128 _upper;
    private readonly U128 _lower;
#else
    private readonly U128 _lower;
    private readonly U128 _upper;
#endif

    /// <summary>Initializes a new instance of the <see cref="U256" /> struct.</summary>
    /// <param name="upper">The upper 128-bits of the 256-bit value.</param>
    /// <param name="lower">The lower 128-bits of the 256-bit value.</param>
    public U256(U128 upper, U128 lower)
    {
        _upper = upper;
        _lower = lower;
    }

    internal U128 Lower => _lower;

    internal U128 Upper => _upper;

    /// <inheritdoc cref="IComparable.CompareTo(object)" />
    public int CompareTo(object? value)
    {
        if (value is U256 other)
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
    public int CompareTo(U256 value)
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
    public static bool operator <(U256 left, U256 right)
    {
        return (left._upper < right._upper)
            || (left._upper == right._upper) && (left._lower < right._lower);
    }

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_GreaterThan(TSelf, TOther)" />
    public static bool operator >(U256 left, U256 right)
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
