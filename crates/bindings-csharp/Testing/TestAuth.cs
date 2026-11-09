namespace SpacetimeDB.Testing;

using System.Text;
using System.Text.Json;

/// <summary>Immutable caller credentials for a test invocation.</summary>
public sealed class TestAuth
{
    private static readonly TestAuth InternalAuth = new(null, null, null);
    private readonly string? _jwtPayload;

    private TestAuth(Identity? sender, ConnectionId? connectionId, string? jwtPayload)
    {
        Sender = sender;
        ConnectionId = connectionId;
        this._jwtPayload = jwtPayload;
    }

    public Identity? Sender { get; }
    public ConnectionId? ConnectionId { get; }

    // Keep lazy parsed claims scoped to each invocation, even when credentials are reused.
    internal AuthCtx SenderAuth =>
        _jwtPayload is null
            ? AuthCtx.InternalForTesting()
            : AuthCtx.FromJwtPayload(_jwtPayload, Sender!.Value);

    /// <summary>Calls as the fixture's database identity, without a connection or JWT.</summary>
    public static TestAuth Internal() => InternalAuth;

    /// <summary>Validates JSON claims and derives the caller identity without verifying a signature.</summary>
    public static TestAuth FromJwtPayload(string jwtPayload, ConnectionId connectionId)
    {
        ArgumentNullException.ThrowIfNull(jwtPayload);
        if (connectionId == default)
            throw new ArgumentException(
                "Client authentication requires a nonzero connection ID.",
                nameof(connectionId)
            );

        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(jwtPayload);
        }
        catch (JsonException error)
        {
            throw new ArgumentException("Invalid JWT payload JSON.", nameof(jwtPayload), error);
        }
        using (document)
        {
            var claims = document.RootElement;
            if (claims.ValueKind != JsonValueKind.Object)
            {
                throw new ArgumentException("JWT payload must be an object.", nameof(jwtPayload));
            }

            var issuer = RequiredClaim(claims, "iss");
            var subject = RequiredClaim(claims, "sub");
            var sender = Native.IdentityFromClaims(issuer, subject);
            if (claims.TryGetProperty("hex_identity", out var hexIdentity))
            {
                if (hexIdentity.ValueKind != JsonValueKind.String)
                {
                    throw new ArgumentException(
                        "JWT 'hex_identity' must be a hex string.",
                        nameof(jwtPayload)
                    );
                }

                Identity claimed;
                try
                {
                    claimed = Identity.FromHexString(hexIdentity.GetString()!);
                }
                catch (Exception error)
                    when (error is ArgumentException or FormatException or InvalidOperationException
                    )
                {
                    throw new ArgumentException(
                        "Invalid JWT 'hex_identity'.",
                        nameof(jwtPayload),
                        error
                    );
                }
                if (claimed != sender)
                {
                    throw new ArgumentException(
                        "JWT 'hex_identity' does not match the issuer and subject.",
                        nameof(jwtPayload)
                    );
                }
            }
            return new TestAuth(sender, connectionId, jwtPayload);
        }
    }

    private static string RequiredClaim(JsonElement claims, string name)
    {
        if (!claims.TryGetProperty(name, out var value) || value.ValueKind != JsonValueKind.String)
        {
            throw new ArgumentException($"JWT '{name}' must be a string.");
        }

        string text;
        try
        {
            text = value.GetString()!;
        }
        catch (InvalidOperationException error)
        {
            throw new ArgumentException($"Invalid JWT '{name}' string.", error);
        }
        return Encoding.UTF8.GetByteCount(text) is 0 or > 128 ? throw new ArgumentException($"JWT '{name}' must contain 1 to 128 UTF-8 bytes.") : text;
    }
}
