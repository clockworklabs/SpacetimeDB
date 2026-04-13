namespace SpacetimeDB.Internal;

using System.Runtime.CompilerServices;

// Note: this utility struct is used by the C# client SDK so it needs to be public.
public readonly struct ByteArrayComparer : IEqualityComparer<byte[]>
{
    public static readonly ByteArrayComparer Instance = new();

    public bool Equals(byte[]? left, byte[]? right)
    {
        if (ReferenceEquals(left, right))
        {
            return true;
        }

        if (left is null || right is null || left.Length != right.Length)
        {
            return false;
        }

        return EqualsUnvectorized(left, right);
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    static bool EqualsUnvectorized(byte[] left, byte[] right)
    {
        for (var i = 0; i < left.Length; i++)
        {
            if (left[i] != right[i])
            {
                return false;
            }
        }

        return true;
    }

    public int GetHashCode(byte[] obj)
    {
        var hash = 17;
        foreach (var b in obj)
        {
            hash = hash * 31 + b;
        }
        return hash;
    }
}
