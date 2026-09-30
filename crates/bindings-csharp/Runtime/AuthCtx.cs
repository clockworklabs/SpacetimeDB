namespace SpacetimeDB;

using System;

public sealed class AuthCtx
{
    private static byte[] jwtBuffer = new byte[0x10_000];

    // Computed on first use, since it needs a host call to read the database's Identity.
    private readonly Lazy<bool> _isInternal;
    private readonly Lazy<JwtClaims?> _jwtLazy;

    private AuthCtx(Func<bool> isInternal, Func<JwtClaims?> jwtFactory)
    {
        _isInternal = new Lazy<bool>(isInternal);
        _jwtLazy = new Lazy<JwtClaims?>(() => jwtFactory?.Invoke());
    }

    internal static readonly AuthCtx Anonymous =
        new(isInternal: static () => false, jwtFactory: static () => null);

    /// <summary>
    /// Create an AuthCtx by looking up the credentials for a connection id in system tables.
    ///
    /// Ideally this would not be part of the public API.
    /// This should only be called inside of a reducer.
    /// </summary>
    public static AuthCtx BuildFromSystemTables(ConnectionId? connectionId, Identity identity)
    {
        // The invocation is internal when its sender is this database.
        Func<bool> isInternal = () =>
            identity == SpacetimeDB.Internal.IReducerContext.GetDatabaseIdentity();
        if (connectionId == null)
        {
            return new AuthCtx(isInternal, jwtFactory: static () => null);
        }
        return FromConnectionId(connectionId.Value, identity, isInternal);
    }

    /// <summary>
    /// Create an AuthCtx that reads JWT for a given connection ID.
    /// </summary>
    private static AuthCtx FromConnectionId(
        ConnectionId connectionId,
        Identity identity,
        Func<bool> isInternal
    )
    {
        return new AuthCtx(
            isInternal,
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
    /// True if the sender of this invocation is this database,
    /// for example in a scheduled reducer or procedure.
    /// False for every other sender, including the database's owner in <c>init</c>.
    /// Equivalent to <c>ctx.Sender == ctx.DatabaseIdentity</c>.
    /// </summary>
    public bool IsInternal => _isInternal.Value;

    /// <summary>
    /// Check if there is a JWT present.
    /// If IsInternal is true, this will be false.
    /// </summary>
    public bool HasJwt => Jwt != null;

    /// <summary>
    /// Load and get the JwtClaims.
    /// Internal invocations have no JWT, even when their sender presented one,
    /// so this is null whenever IsInternal is true.
    /// </summary>
    public JwtClaims? Jwt => IsInternal ? null : _jwtLazy.Value;
}
