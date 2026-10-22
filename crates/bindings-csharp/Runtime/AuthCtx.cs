namespace SpacetimeDB;

using System;

public sealed class AuthCtx
{
    private static byte[] jwtBuffer = new byte[0x10_000];

    // Computed on first use, since comparing the sender with the database's Identity needs a host call.
    private readonly Lazy<bool> _isInternal;
    private readonly Lazy<JwtClaims?> _jwtLazy;

    private AuthCtx(Identity sender, Func<JwtClaims?> jwtFactory)
    {
        _isInternal = new Lazy<bool>(
            () => sender == SpacetimeDB.Internal.IReducerContext.GetDatabaseIdentity()
        );
        _jwtLazy = new Lazy<JwtClaims?>(() => jwtFactory?.Invoke());
    }

    /// <summary>
    /// Create an AuthCtx by looking up the credentials for a connection id in system tables.
    ///
    /// Ideally this would not be part of the public API.
    /// This should only be called inside of a reducer.
    /// </summary>
    public static AuthCtx BuildFromSystemTables(ConnectionId? connectionId, Identity identity)
    {
        if (connectionId == null)
        {
            return new AuthCtx(identity, jwtFactory: static () => null);
        }
        return FromConnectionId(connectionId.Value, identity);
    }

    /// <summary>
    /// Create an AuthCtx that reads JWT for a given connection ID.
    /// </summary>
    private static AuthCtx FromConnectionId(ConnectionId connectionId, Identity identity)
    {
        return new AuthCtx(
            identity,
            jwtFactory: () =>
            {
                var result = SpacetimeDB.Internal.FFI.get_jwt(ref connectionId, out var source);
                SpacetimeDB.Internal.FFI.CheckedStatus.Marshaller.ConvertToManaged(result);
                var jwt = ReadJwt(source);
                return jwt == null ? null : new JwtClaims(jwt, identity);
            }
        );
    }

    /// <summary>
    /// Create an AuthCtx with the given JWT payload,
    /// as for the sender of an HTTP handler, which has no connection ID to look it up by.
    /// </summary>
    internal static AuthCtx FromJwt(string? jwt, Identity identity)
    {
        return new AuthCtx(
            identity,
            jwtFactory: () => jwt == null ? null : new JwtClaims(jwt, identity)
        );
    }

    /// <summary>
    /// Read the JWT payload that the host wrote to a bytes source.
    /// </summary>
    internal static string? ReadJwt(SpacetimeDB.Internal.BytesSource source)
    {
        using var stream = SpacetimeDB.Internal.Module.Consume(source, ref jwtBuffer);
        if (stream.Length == 0)
        {
            return null;
        }
        return System.Text.Encoding.UTF8.GetString(jwtBuffer, 0, checked((int)stream.Length));
    }

    /// <summary>
    /// True if the sender of this invocation is this database, as for scheduled reducers.
    /// False in init, whose sender is the owner.
    /// </summary>
    public bool IsInternal => _isInternal.Value;

    /// <summary>
    /// Check if there is a JWT present.
    /// If IsInternal is true, this will be false.
    /// </summary>
    public bool HasJwt
    {
        get
        {
            if (IsInternal)
            {
                return false;
            }

            // At this point we do load the bytes.
            return _jwtLazy.Value != null;
        }
    }

    /// <summary>
    /// Load and get the JwtClaims.
    /// If IsInternal is true, this will be null.
    /// </summary>
    public JwtClaims? Jwt => IsInternal ? null : _jwtLazy.Value;
}
