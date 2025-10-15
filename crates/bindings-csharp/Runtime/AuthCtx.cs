namespace SpacetimeDB;
using SpacetimeDB.Internal;

using System;

public sealed class AuthCtx
{
    private readonly bool _isInternal;
    private readonly Lazy<JwtClaims?> _jwtLazy;

    private AuthCtx(bool isInternal, Func<JwtClaims?> jwtFactory)
    {
        _isInternal = isInternal;
        _jwtLazy = new Lazy<JwtClaims?>(() => jwtFactory?.Invoke());
    }

    /// <summary>
    /// Create an AuthCtx for an internal call, with no JWT.
    /// </summary>
    private static AuthCtx Internal()
    {
        return new AuthCtx(isInternal: true, jwtFactory: () => null);
    }

    /// <summary>
    /// Create an AuthCtx from a raw JWT payload (JSON claims).
    /// </summary>
    private static AuthCtx FromJwtPayload(string jwtPayload, Identity identity)
    {
        return new AuthCtx(isInternal: false, jwtFactory: () => new JwtClaims(jwtPayload, identity));
    }

    private static AuthCtx FromOptionalConnectionId(ConnectionId? connectionId, Identity identity)
    {
        if (connectionId == null)
        {
            return Internal();
        }
        return FromConnectionId(connectionId.Value, identity);
    }

    /// <summary>
    /// Create an AuthCtx that reads JWT for a given connection ID.
    /// Equivalent to Rust's `from_connection_id`.
    /// </summary>
    public static AuthCtx FromConnectionId(ConnectionId connectionId, Identity identity)
    {
        return new AuthCtx(isInternal: false, jwtFactory: () =>
        {
            var result = SpacetimeDB.Internal.FFI.get_jwt(ref connectionId, out var source);
            SpacetimeDB.Internal.FFI.CheckedStatus.Marshaller.ConvertToManaged(result);
            var bytes = SpacetimeDB.Internal.Module.Consume(source);
            if (bytes == null || bytes.Length == 0)
            {
                return null;
            }
            var jwt = System.Text.Encoding.UTF8.GetString(bytes);
            return jwt != null ? new JwtClaims(jwt, identity) : null;
        });
    }

    /// <summary>
    /// True if this reducer was spawned from inside the database.
    /// </summary>
    public bool IsInternal => _isInternal;

    /// <summary>
    /// Check if there is a JWT without loading it.
    /// If IsInternal is true, this will be false.
    /// </summary>
    public bool HasJwt
    {
        get
        {
            if (_isInternal)
            {
                return false;
            }

            // At this point we do load the bytes.
            return _jwtLazy.Value != null;
        }
    }

    /// <summary>
    /// Load and get the JwtClaims.
    /// </summary>
    public JwtClaims? Jwt => _jwtLazy.Value;
}
