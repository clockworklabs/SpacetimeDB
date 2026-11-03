namespace SpacetimeDB;

/// <summary>
/// Read-only database environment. Values are plaintext and accessible to database
/// collaborators. Procedure reads outside a transaction use a short snapshot.
/// </summary>
public readonly struct DatabaseEnvironment
{
    internal static readonly DatabaseEnvironment Instance = new();

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    private readonly Internal.DatabaseAccess? access;

    internal DatabaseEnvironment(Internal.DatabaseAccess? access) => this.access = access;
#endif

    /// <summary>Return null for a missing key, or an empty string for a present empty value.</summary>
    public unsafe string? Get(string key)
    {
        ArgumentNullException.ThrowIfNull(key);
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        var owner = Internal.DatabaseAccess.Require(access);
        owner.CheckActive();
        return owner.Backend.GetEnv(key);
#else
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
#endif
    }
}
