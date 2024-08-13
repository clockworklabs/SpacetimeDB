namespace SpacetimeDB;

using System;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;

/// <summary>Represents a 128-bit unsigned integer.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct U256 : IBigInt<U256>
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

    /// <inheritdoc cref="IComparable.CompareTo(object)" />
    public int CompareTo(object? value) => BigIntHelpers.CompareTo(this, value);

    /// <inheritdoc cref="IComparable{T}.CompareTo(T)" />
    public int CompareTo(U256 value)
    {
        var cmp = _upper.CompareTo(value._upper);
        return cmp != 0 ? cmp : _lower.CompareTo(value._lower);
    }

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_LessThan(TSelf, TOther)" />
    public static bool operator <(U256 left, U256 right) => left.CompareTo(right) < 0;

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_GreaterThan(TSelf, TOther)" />
    public static bool operator >(U256 left, U256 right) => left.CompareTo(right) > 0;

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_LessThanEqual(TSelf, TOther)" />
    public static bool operator <=(U256 left, U256 right) => left.CompareTo(right) <= 0;

    /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_GreaterThanEqual(TSelf, TOther)" />
    public static bool operator >=(U256 left, U256 right) => left.CompareTo(right) >= 0;

    /// <inheritdoc cref="object.ToString()" />
    public override string ToString() => BigIntHelpers.ToString(this, true);
}
