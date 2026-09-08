namespace SpacetimeDB;

/// <summary>
/// Read-only database environment. Values are plaintext and accessible to database
/// collaborators. Procedure reads outside a transaction use a short snapshot.
/// </summary>
public readonly struct DatabaseEnvironment
{
    internal static readonly DatabaseEnvironment Instance = new();

    /// <summary>Return null for a missing key, or an empty string for a present empty value.</summary>
    public unsafe string? Get(string key)
    {
        ArgumentNullException.ThrowIfNull(key);
        var bytes = System.Text.Encoding.UTF8.GetBytes(key);
        fixed (byte* ptr = bytes)
        {
            Internal.FFI.env_get(ptr, checked((uint)bytes.Length), out var source);
            if (source == Internal.BytesSource.INVALID)
            {
                return null;
            }
            return System.Text.Encoding.UTF8.GetString(Internal.Module.Consume(source));
        }
    }
}
