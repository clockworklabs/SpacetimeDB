using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using System;

namespace SpacetimeDB
{
    /// <summary>Represents a 128-bit unsigned integer.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public readonly struct UInt256 : IEquatable<UInt256>
    {
        internal const int Size = 32;

#if BIGENDIAN
        private readonly UInt128 _upper;
        private readonly UInt128 _lower;
#else
        private readonly UInt128 _lower;
        private readonly UInt128 _upper;
#endif

        /// <summary>Initializes a new instance of the <see cref="UInt256" /> struct.</summary>
        /// <param name="upper">The upper 128-bits of the 256-bit value.</param>
        /// <param name="lower">The lower 128-bits of the 256-bit value.</param>
        public UInt256(UInt128 upper, UInt128 lower)
        {
            _lower = lower;
            _upper = upper;
        }

        internal UInt128 Lower => _lower;

        internal UInt128 Upper => _upper;

        /// <inheritdoc cref="IComparable.CompareTo(object)" />
        public int CompareTo(object? value)
        {
            if (value is UInt256 other)
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
        public int CompareTo(UInt256 value)
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
        public static bool operator <(UInt256 left, UInt256 right)
        {
            return (left._upper < right._upper)
                || (left._upper == right._upper) && (left._lower < right._lower);
        }

        /// <inheritdoc cref="IComparisonOperators{TSelf, TOther, TResult}.op_GreaterThan(TSelf, TOther)" />
        public static bool operator >(UInt256 left, UInt256 right)
        {
            return (left._upper > right._upper)
                || (left._upper == right._upper) && (left._lower > right._lower);
        }

        //
        // IEqualityOperators
        //

        /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Equality(TSelf, TOther)" />
        public static bool operator ==(UInt256 left, UInt256 right) => (left._lower == right._lower) && (left._upper == right._upper);

        /// <inheritdoc cref="IEqualityOperators{TSelf, TOther, TResult}.op_Inequality(TSelf, TOther)" />
        public static bool operator !=(UInt256 left, UInt256 right) => (left._lower != right._lower) || (left._upper != right._upper);

        /// <inheritdoc cref="object.Equals(object?)" />
        public override bool Equals([NotNullWhen(true)] object? obj)
        {
            return (obj is UInt256 other) && Equals(other);
        }

        /// <inheritdoc cref="IEquatable{T}.Equals(T)" />
        public bool Equals(UInt256 x) => _upper == x._upper && _lower == x._lower;

        /// <inheritdoc cref="object.GetHashCode()" />
        public override int GetHashCode() => HashCode.Combine(_lower, _upper);

        /// <inheritdoc cref="object.ToString()" />
        public override string ToString() => $"UInt256({_upper},{_lower})";
    }
}
