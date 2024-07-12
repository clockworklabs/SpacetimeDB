// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.

#if !NET7_0_OR_GREATER
using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;

namespace System
{
    /// <summary>Represents a 128-bit signed integer.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public readonly struct Int128 : IEquatable<Int128>
    {
        internal const int Size = 16;

#if BIGENDIAN
        private readonly ulong _upper;
        private readonly ulong _lower;
#else
        private readonly ulong _lower;
        private readonly ulong _upper;
#endif

        /// <summary>Initializes a new instance of the <see cref="Int128" /> struct.</summary>
        /// <param name="upper">The upper 64-bits of the 128-bit value.</param>
        /// <param name="lower">The lower 64-bits of the 128-bit value.</param>
        public Int128(ulong upper, ulong lower)
        {
            _lower = lower;
            _upper = upper;
        }

        internal ulong Lower => _lower;

        internal ulong Upper => _upper;

        /// <inheritdoc cref="IComparable.CompareTo(object)" />
        public int CompareTo(object? value)
        {
            if (value is Int128 other)
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
        public int CompareTo(Int128 value)
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
        public static bool operator <(Int128 left, Int128 right)
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
        public static bool operator >(Int128 left, Int128 right)
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
        public static bool IsNegative(Int128 value) => (long)value._upper < 0;

        //
        // IEqualityOperators
        //

        /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Equality(TSelf, TOther)" />
        public static bool operator ==(Int128 left, Int128 right) => (left._lower == right._lower) && (left._upper == right._upper);

        /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Inequality(TSelf, TOther)" />
        public static bool operator !=(Int128 left, Int128 right) => (left._lower != right._lower) || (left._upper != right._upper);

        /// <inheritdoc cref="object.Equals(object?)" />
        public override bool Equals([NotNullWhen(true)] object? obj)
        {
            return (obj is Int128 other) && Equals(other);
        }

        /// <inheritdoc cref="IEquatable{T}.Equals(T)" />
        public bool Equals(Int128 x) => _upper == x._upper && _lower == x._lower;

        /// <inheritdoc cref="object.GetHashCode()" />
        public override int GetHashCode() => HashCode.Combine(_lower, _upper);

        /// <inheritdoc cref="object.ToString()" />
        public override string ToString() => $"Int128({_upper},{_lower})";
    }
}
#endif
