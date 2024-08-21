namespace SpacetimeDB;

using System;
using System.Diagnostics.CodeAnalysis;
using System.Numerics;
using System.Runtime.InteropServices;

/// <summary>Represents a 128-bit unsigned integer.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly struct U256 : IEquatable<U256>, IComparable, IComparable<U256>
{
    internal const int Size = 32;

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
            throw new ArgumentException("Argument must be a U256", nameof(value));
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

    //
    // IEqualityOperators
    //

    /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Equality(TSelf, TOther)" />
    public static bool operator ==(U256 left, U256 right) =>
        (left._lower == right._lower) && (left._upper == right._upper);

    /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Inequality(TSelf, TOther)" />
    public static bool operator !=(U256 left, U256 right) =>
        (left._lower != right._lower) || (left._upper != right._upper);

    /// <inheritdoc cref="object.Equals(object?)" />
    public override bool Equals([NotNullWhen(true)] object? obj)
    {
        return (obj is U256 other) && Equals(other);
    }

    /// <inheritdoc cref="IEquatable{T}.Equals(T)" />
    public bool Equals(U256 x) => _upper == x._upper && _lower == x._lower;

    /// <inheritdoc cref="object.GetHashCode()" />
    public override int GetHashCode() => HashCode.Combine(_lower, _upper);

    private BigInteger AsBigInt() =>
        new(
            MemoryMarshal.AsBytes(stackalloc[] { this }),
            isUnsigned: false,
            isBigEndian: !BitConverter.IsLittleEndian
        );

    /// <inheritdoc cref="object.ToString()" />
    public override string ToString() => AsBigInt().ToString();
}
