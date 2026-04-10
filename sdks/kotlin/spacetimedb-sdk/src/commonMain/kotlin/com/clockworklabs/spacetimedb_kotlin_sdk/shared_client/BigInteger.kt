package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Sign of a BigInteger magnitude used with [BigInteger.fromByteArray].
 */
public enum class Sign { POSITIVE, NEGATIVE, ZERO }

/**
 * A fixed-width big integer backed by a canonical little-endian two's complement [ByteArray].
 *
 * Designed for fast construction from BSATN wire bytes (which are already LE),
 * avoiding the allocation overhead of arbitrary-precision libraries.
 */
public class BigInteger private constructor(
    // Canonical LE two's complement bytes. Always at least 1 byte.
    // Canonical means no redundant sign-extension bytes at the high end.
    internal val leBytes: ByteArray
) : Comparable<BigInteger> {

    /** Constructs a BigInteger from a [Long] value. */
    public constructor(value: Long) : this(longToLeBytes(value))

    /** Constructs a BigInteger from an [Int] value. */
    public constructor(value: Int) : this(value.toLong())

    // ---- Companion: constants and factories ----

    public companion object {
        private val HEX_CHARS = "0123456789abcdef".toCharArray()

        /** The BigInteger constant zero. */
        public val ZERO: BigInteger = BigInteger(byteArrayOf(0))

        /** The BigInteger constant one. */
        public val ONE: BigInteger = BigInteger(1L)

        /** The BigInteger constant two. */
        public val TWO: BigInteger = BigInteger(2L)

        /** The BigInteger constant ten. */
        public val TEN: BigInteger = BigInteger(10L)

        /**
         * Parses a string representation of a BigInteger in the given [radix].
         * Supports radix 10 (decimal) and 16 (hexadecimal). Negative values use a leading '-'.
         */
        public fun parseString(value: String, radix: Int = 10): BigInteger {
            require(value.isNotEmpty()) { "Empty string" }
            return when (radix) {
                10 -> parseDecimal(value)
                16 -> parseHex(value)
                else -> throw IllegalArgumentException("Unsupported radix: $radix")
            }
        }

        /** Creates a BigInteger from an unsigned [ULong] value. */
        public fun fromULong(value: ULong): BigInteger {
            if (value == 0UL) return ZERO
            val bytes = ByteArray(8)
            var v = value
            for (i in 0 until 8) {
                bytes[i] = (v and 0xFFu).toByte()
                v = v shr 8
            }
            // If bit 63 is set, the byte would look negative in two's complement; add sign byte
            val leBytes = if (bytes[7].toInt() and 0x80 != 0) {
                bytes.copyOf(9) // extra byte is 0x00
            } else {
                bytes
            }
            return BigInteger(canonicalize(leBytes))
        }

        /**
         * Creates a BigInteger from a big-endian unsigned magnitude byte array and a [sign].
         * This matches the ionspin `BigInteger.fromByteArray(bytes, sign)` contract.
         */
        public fun fromByteArray(bytes: ByteArray, sign: Sign): BigInteger {
            if (sign == Sign.ZERO || bytes.all { it == 0.toByte() }) return ZERO

            // Reverse BE magnitude to LE
            val le = bytes.reversedArray()

            // Ensure non-negative two's complement (add 0x00 sign byte if high bit set)
            val positive = if (le.last().toInt() and 0x80 != 0) {
                le.copyOf(le.size + 1)
            } else {
                le
            }

            val canonical = canonicalize(positive)
            return if (sign == Sign.NEGATIVE) {
                BigInteger(canonical).unaryMinus()
            } else {
                BigInteger(canonical)
            }
        }

        /**
         * Constructs a BigInteger from LE two's complement bytes (signed interpretation).
         * Used by BsatnReader for signed integer types (I128, I256).
         */
        internal fun fromLeBytes(source: ByteArray, offset: Int, length: Int): BigInteger {
            val bytes = source.copyOfRange(offset, offset + length)
            return BigInteger(canonicalize(bytes))
        }

        /**
         * Constructs a non-negative BigInteger from LE bytes (unsigned interpretation).
         * If the high bit is set, a zero byte is appended to force a positive two's complement value.
         * Used by BsatnReader for unsigned integer types (U128, U256).
         */
        internal fun fromLeBytesUnsigned(source: ByteArray, offset: Int, length: Int): BigInteger {
            val bytes = source.copyOfRange(offset, offset + length)
            val unsigned = if (bytes[length - 1].toInt() and 0x80 != 0) {
                bytes.copyOf(length + 1) // extra 0x00 forces non-negative
            } else {
                bytes
            }
            return BigInteger(canonicalize(unsigned))
        }

        // ---- Internal helpers ----

        private fun longToLeBytes(value: Long): ByteArray {
            val bytes = ByteArray(8)
            var v = value
            for (i in 0 until 8) {
                bytes[i] = (v and 0xFF).toByte()
                v = v shr 8
            }
            return canonicalize(bytes)
        }

        /**
         * Strips redundant sign-extension bytes from the high end of LE two's complement bytes.
         * Returns a minimal representation (at least 1 byte).
         */
        internal fun canonicalize(bytes: ByteArray): ByteArray {
            if (bytes.isEmpty()) return byteArrayOf(0)
            var len = bytes.size
            val isNegative = bytes[len - 1].toInt() and 0x80 != 0
            val signExt = if (isNegative) 0xFF.toByte() else 0x00.toByte()

            while (len > 1) {
                if (bytes[len - 1] != signExt) break
                // Can only strip if the next byte preserves the sign
                if ((bytes[len - 2].toInt() and 0x80 != 0) != isNegative) break
                len--
            }
            return if (len == bytes.size) bytes else bytes.copyOfRange(0, len)
        }

        /** Sign-extends LE bytes to the given [size]. */
        private fun signExtend(bytes: ByteArray, size: Int): ByteArray {
            if (size <= bytes.size) return bytes
            val result = bytes.copyOf(size)
            if (bytes.last().toInt() and 0x80 != 0) {
                for (i in bytes.size until size) result[i] = 0xFF.toByte()
            }
            return result
        }

        private fun parseDecimal(str: String): BigInteger {
            val isNeg = str.startsWith('-')
            val digits = if (isNeg) str.substring(1) else str
            require(digits.isNotEmpty() && digits.all { it in '0'..'9' }) {
                "Invalid decimal string: $str"
            }

            var magnitude = byteArrayOf(0) // LE unsigned magnitude
            for (ch in digits) {
                magnitude = multiplyByAndAdd(magnitude, 10, ch - '0')
            }

            // Ensure the magnitude is positive in two's complement
            if (magnitude.last().toInt() and 0x80 != 0) {
                magnitude = magnitude.copyOf(magnitude.size + 1) // add 0x00 sign byte
            }

            val canonical = canonicalize(magnitude)
            return if (isNeg && !(canonical.size == 1 && canonical[0] == 0.toByte())) {
                BigInteger(canonical).unaryMinus()
            } else {
                BigInteger(canonical)
            }
        }

        private fun parseHex(str: String): BigInteger {
            val isNeg = str.startsWith('-')
            val hexStr = if (isNeg) str.substring(1) else str
            require(hexStr.isNotEmpty() && hexStr.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) {
                "Invalid hex string: $str"
            }

            // Pad to even length, convert to BE bytes
            val padded = if (hexStr.length % 2 != 0) "0$hexStr" else hexStr
            val beBytes = ByteArray(padded.length / 2) { i ->
                padded.substring(i * 2, i * 2 + 2).toInt(16).toByte()
            }

            // Reverse to LE
            val le = beBytes.reversedArray()

            // Ensure non-negative two's complement
            val positive = if (le.isNotEmpty() && le.last().toInt() and 0x80 != 0) {
                le.copyOf(le.size + 1)
            } else {
                le
            }

            val canonical = canonicalize(positive)
            return if (isNeg && !(canonical.size == 1 && canonical[0] == 0.toByte())) {
                BigInteger(canonical).unaryMinus()
            } else {
                BigInteger(canonical)
            }
        }

        /**
         * Multiplies an unsigned LE magnitude by [factor] and adds [addend].
         * Returns a new array one byte larger to accommodate overflow.
         */
        private fun multiplyByAndAdd(bytes: ByteArray, factor: Int, addend: Int): ByteArray {
            val result = ByteArray(bytes.size + 1)
            var carry = addend
            for (i in bytes.indices) {
                val v = (bytes[i].toInt() and 0xFF) * factor + carry
                result[i] = (v and 0xFF).toByte()
                carry = v shr 8
            }
            result[bytes.size] = (carry and 0xFF).toByte()
            return result
        }
    }

    // ---- Arithmetic ----

    /** Returns the sum of this and [other]. */
    public fun add(other: BigInteger): BigInteger {
        val maxLen = maxOf(leBytes.size, other.leBytes.size) + 1
        val a = signExtend(leBytes, maxLen)
        val b = signExtend(other.leBytes, maxLen)

        val result = ByteArray(maxLen)
        var carry = 0
        for (i in 0 until maxLen) {
            val sum = (a[i].toInt() and 0xFF) + (b[i].toInt() and 0xFF) + carry
            result[i] = (sum and 0xFF).toByte()
            carry = sum shr 8
        }
        return BigInteger(canonicalize(result))
    }

    public operator fun plus(other: BigInteger): BigInteger = add(other)
    public operator fun minus(other: BigInteger): BigInteger = add(-other)

    /** Returns the two's complement negation of this value. */
    public operator fun unaryMinus(): BigInteger {
        if (signum() == 0) return this
        // Sign-extend by 1 byte to handle overflow (e.g., negating -128 needs 9 bits for +128)
        val extended = signExtend(leBytes, leBytes.size + 1)
        // Invert all bits
        for (i in extended.indices) {
            extended[i] = extended[i].toInt().inv().toByte()
        }
        // Add 1
        var carry = 1
        for (i in extended.indices) {
            val sum = (extended[i].toInt() and 0xFF) + carry
            extended[i] = (sum and 0xFF).toByte()
            carry = sum shr 8
            if (carry == 0) break
        }
        return BigInteger(canonicalize(extended))
    }

    /** Left-shifts this value by [n] bits. */
    public fun shl(n: Int): BigInteger {
        require(n >= 0) { "Shift amount must be non-negative: $n" }
        if (n == 0 || signum() == 0) return this

        val byteShift = n / 8
        val bitShift = n % 8

        // Allocate: original size + byte shift + 1 for bit overflow
        val newSize = leBytes.size + byteShift + 1
        val result = ByteArray(newSize)

        // Copy original bytes at the shifted position
        leBytes.copyInto(result, byteShift)

        // Sign-extend the high bytes beyond the original data
        if (signum() < 0) {
            for (i in leBytes.size + byteShift until newSize) {
                result[i] = 0xFF.toByte()
            }
        }

        // Apply bit shift
        if (bitShift > 0) {
            var carry = 0
            for (i in byteShift until newSize) {
                val v = ((result[i].toInt() and 0xFF) shl bitShift) or carry
                result[i] = (v and 0xFF).toByte()
                carry = (v shr 8) and 0xFF
            }
        }

        return BigInteger(canonicalize(result))
    }

    // ---- Properties ----

    /** Returns -1, 0, or 1 as this value is negative, zero, or positive. */
    public fun signum(): Int {
        val isNeg = leBytes.last().toInt() and 0x80 != 0
        if (isNeg) return -1
        // Check if all bytes are zero
        for (b in leBytes) {
            if (b != 0.toByte()) return 1
        }
        return 0
    }

    /** Returns true if this value fits in [n] bytes of signed two's complement. */
    internal fun fitsInSignedBytes(n: Int): Boolean = leBytes.size <= n

    /** Returns true if this non-negative value fits in [n] bytes of unsigned representation. */
    internal fun fitsInUnsignedBytes(n: Int): Boolean {
        if (signum() < 0) return false
        // Canonical positive value may have a trailing 0x00 sign byte.
        // The unsigned magnitude is leBytes without that trailing sign byte.
        return leBytes.size <= n ||
            (leBytes.size == n + 1 && leBytes[n] == 0.toByte())
    }

    // ---- Conversion ----

    /**
     * Returns the big-endian two's complement byte array representation.
     * This matches the convention of `java.math.BigInteger.toByteArray()`.
     */
    public fun toByteArray(): ByteArray = leBytes.reversedArray()

    /**
     * Returns the big-endian two's complement byte array representation.
     * Alias for [toByteArray] for compatibility with ionspin's extension function.
     */
    public fun toTwosComplementByteArray(): ByteArray = toByteArray()

    /**
     * Returns LE bytes at exactly [size] bytes, sign-extending or truncating as needed.
     * Used for efficient BSATN writing and Identity/ConnectionId.toByteArray().
     */
    internal fun toLeBytesFixedWidth(size: Int): ByteArray {
        val result = ByteArray(size)
        writeLeBytes(result, 0, size)
        return result
    }

    /**
     * Writes LE bytes directly into [dest] at [destOffset], padded with sign extension to [size] bytes.
     * Zero-allocation write path for BsatnWriter.
     */
    internal fun writeLeBytes(dest: ByteArray, destOffset: Int, size: Int) {
        val copyLen = minOf(leBytes.size, size)
        leBytes.copyInto(dest, destOffset, 0, copyLen)
        if (copyLen < size) {
            val padByte = if (signum() < 0) 0xFF.toByte() else 0x00.toByte()
            for (i in copyLen until size) {
                dest[destOffset + i] = padByte
            }
        }
    }

    /** Returns the decimal string representation. */
    override fun toString(): String = toStringRadix(10)

    /** Returns the string representation in the given [radix] (10 or 16). */
    public fun toString(radix: Int): String = toStringRadix(radix)

    private fun toStringRadix(radix: Int): String = when (radix) {
        10 -> toDecimalString()
        16 -> toHexString()
        else -> throw IllegalArgumentException("Unsupported radix: $radix")
    }

    private fun toDecimalString(): String {
        val sign = signum()
        if (sign == 0) return "0"

        val isNeg = sign < 0
        // Work on a copy of the unsigned magnitude
        val magnitude = if (isNeg) (-this).leBytes.copyOf() else leBytes.copyOf()

        val digits = StringBuilder()
        while (!isAllZero(magnitude)) {
            val remainder = divideByTenInPlace(magnitude)
            digits.append(('0' + remainder))
        }

        if (isNeg) digits.append('-')
        return digits.reverse().toString()
    }

    private fun toHexString(): String {
        val sign = signum()
        if (sign == 0) return "0"
        if (sign < 0) return "-" + (-this).toHexString()

        val sb = StringBuilder()
        var leading = true
        for (i in leBytes.size - 1 downTo 0) {
            val b = leBytes[i].toInt() and 0xFF
            val hi = b shr 4
            val lo = b and 0x0F
            if (leading) {
                if (hi != 0) {
                    sb.append(HEX_CHARS[hi])
                    sb.append(HEX_CHARS[lo])
                    leading = false
                } else if (lo != 0) {
                    sb.append(HEX_CHARS[lo])
                    leading = false
                }
            } else {
                sb.append(HEX_CHARS[hi])
                sb.append(HEX_CHARS[lo])
            }
        }
        return if (sb.isEmpty()) "0" else sb.toString()
    }

    // ---- Comparison and equality ----

    override fun compareTo(other: BigInteger): Int {
        val thisSign = signum()
        val otherSign = other.signum()

        if (thisSign != otherSign) return thisSign.compareTo(otherSign)
        if (thisSign == 0) return 0

        // Same sign: sign-extend to equal length and compare from MSB
        val maxLen = maxOf(leBytes.size, other.leBytes.size)
        val a = signExtend(leBytes, maxLen)
        val b = signExtend(other.leBytes, maxLen)

        for (i in maxLen - 1 downTo 0) {
            val av = a[i].toInt() and 0xFF
            val bv = b[i].toInt() and 0xFF
            if (av != bv) return av.compareTo(bv)
        }
        return 0
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is BigInteger) return false
        return leBytes.contentEquals(other.leBytes)
    }

    override fun hashCode(): Int = leBytes.contentHashCode()

    // ---- Private helpers ----

    private fun isAllZero(bytes: ByteArray): Boolean {
        for (b in bytes) if (b != 0.toByte()) return false
        return true
    }

    /**
     * Divides the unsigned LE magnitude in-place by 10 and returns the remainder (0-9).
     * Processes from MSB (highest index) to LSB for schoolbook division.
     */
    private fun divideByTenInPlace(bytes: ByteArray): Int {
        var carry = 0
        for (i in bytes.size - 1 downTo 0) {
            val cur = carry * 256 + (bytes[i].toInt() and 0xFF)
            bytes[i] = (cur / 10).toByte()
            carry = cur % 10
        }
        return carry
    }
}
