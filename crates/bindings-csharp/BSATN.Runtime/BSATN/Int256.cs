// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.

using System;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;

namespace SpacetimeDB
{
    /// <summary>Represents a 256-bit signed integer.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public readonly struct Int256 : IEquatable<Int256>
    {
        internal const int Size = 32;

#if BIGENDIAN
        private readonly UInt128 _upper;
        private readonly UInt128 _lower;
#else
        private readonly UInt128 _lower;
        private readonly UInt128 _upper;
#endif

        /// <summary>Initializes a new instance of the <see cref="Int256" /> struct.</summary>
        /// <param name="upper">The upper 128-bits of the 256-bit value.</param>
        /// <param name="lower">The lower 128-bits of the 256-bit value.</param>
        public Int256(UInt128 upper, UInt128 lower)
        {
            _lower = lower;
            _upper = upper;
        }

        internal UInt128 Lower => _lower;

        internal UInt128 Upper => _upper;

        /// <inheritdoc cref="IComparable.CompareTo(object)" />
        public int CompareTo(object? value)
        {
            if (value is Int256 other)
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
        public int CompareTo(Int256 value)
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
        public static bool operator <(Int256 left, Int256 right)
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
        public static bool operator >(Int256 left, Int256 right)
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

        public static bool IsNegative(Int256 value) =>
#if NET7_0_OR_GREATER
            ((long)(value._upper >> 64))
#else
            (long)value._upper.Upper
#endif
            < 0;

        //
        // IEqualityOperators
        //

        /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Equality(TSelf, TOther)" />
        public static bool operator ==(Int256 left, Int256 right) => (left._lower == right._lower) && (left._upper == right._upper);

        /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Inequality(TSelf, TOther)" />
        public static bool operator !=(Int256 left, Int256 right) => (left._lower != right._lower) || (left._upper != right._upper);

        /// <inheritdoc cref="object.Equals(object?)" />
        public override bool Equals([NotNullWhen(true)] object? obj)
        {
            return (obj is Int256 other) && Equals(other);
        }

        /// <inheritdoc cref="IEquatable{T}.Equals(T)" />
        public bool Equals(Int256 x) => _upper == x._upper && _lower == x._lower;

        /// <inheritdoc cref="object.GetHashCode()" />
        public override int GetHashCode() => HashCode.Combine(_lower, _upper);

        /// <inheritdoc cref="object.ToString()" />
        public override string ToString() => $"Int256({_upper},{_lower})";
    }
}
