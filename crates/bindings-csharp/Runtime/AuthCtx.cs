namespace SpacetimeDB;

using System;

public sealed class AuthCtx
{
    private static byte[] jwtBuffer = new byte[0x10_000];

    private readonly bool _isInternal;
    private readonly Lazy<JwtClaims?> _jwtLazy;

    private AuthCtx(bool isInternal, Func<JwtClaims?> jwtFactory)
    {
        _isInternal = isInternal;
        _jwtLazy = new Lazy<JwtClaims?>(() => jwtFactory?.Invoke());
    }

    /// <summary>
    /// Capture verified invocation authority independently from lazy JWT loading.
    /// </summary>
    internal static AuthCtx FromVerifiedCall(uint callAuthFlags, Func<JwtClaims?> jwtFactory) =>
        new(isInternal: (callAuthFlags & 1) != 0, jwtFactory);

    /// <summary>
    /// Create an AuthCtx by looking up the credentials for a connection id in system tables.
    ///
    /// Ideally this would not be part of the public API.
    /// This should only be called inside of a reducer.
    /// </summary>
    public static AuthCtx BuildFromSystemTables(ConnectionId? connectionId, Identity identity)
    {
        // Read synchronously while this invocation is active. Neither connection
        // presence nor token claims determine internal authority.
        var callAuthFlags = SpacetimeDB.Internal.FFI.get_call_auth_flags();
        if (connectionId == null)
        {
            return FromVerifiedCall(callAuthFlags, () => null);
        }
        return FromConnectionId(connectionId.Value, identity, callAuthFlags);
    }

    /// <summary>
    /// Create an AuthCtx that reads JWT for a given connection ID.
    /// </summary>
    private static AuthCtx FromConnectionId(
        ConnectionId connectionId,
        Identity identity,
        uint callAuthFlags
    )
    {
        return FromVerifiedCall(
            callAuthFlags,
            jwtFactory: () =>
            {
                var result = SpacetimeDB.Internal.FFI.get_jwt(ref connectionId, out var source);
                SpacetimeDB.Internal.FFI.CheckedStatus.Marshaller.ConvertToManaged(result);
                using var stream = SpacetimeDB.Internal.Module.Consume(source, ref jwtBuffer);
                if (stream.Length == 0)
                {
                    return null;
                }
                var jwt = System.Text.Encoding.UTF8.GetString(
                    jwtBuffer,
                    0,
                    checked((int)stream.Length)
                );
                return new JwtClaims(jwt, identity);
            }
        );
    }

    /// <summary>
    /// True if the host verified internal authority for this invocation.
    /// </summary>
    public bool IsInternal => _isInternal;

    /// <summary>
    /// Check if there is a JWT present.
    /// Independent of IsInternal. An internal call may also have a JWT.
    /// </summary>
    public bool HasJwt
    {
        get
        {
            // At this point we do load the bytes.
            return _jwtLazy.Value != null;
        }
    }

    /// <summary>
    /// Load and get the JwtClaims.
    /// </summary>
    public JwtClaims? Jwt => _jwtLazy.Value;
}
