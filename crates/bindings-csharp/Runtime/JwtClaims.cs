namespace SpacetimeDB;

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

public sealed class JwtClaims
{
    private readonly string _payload;
    private readonly Lazy<JsonDocument> _parsed;
    private readonly Lazy<List<string>> _audience;

    public JwtClaims(string jwt)
    {
        _payload = jwt ?? throw new ArgumentNullException(nameof(jwt));
        _parsed = new Lazy<JsonDocument>(() =>
            JsonDocument.Parse(_payload)
        );
        _audience = new Lazy<List<string>>(ExtractAudience);
    }

    private JsonDocument Parsed => _parsed.Value;

    private JsonElement RootElement => Parsed.RootElement;

    public string Subject
    {
        get
        {
            if (RootElement.TryGetProperty("sub", out var sub) && sub.ValueKind == JsonValueKind.String)
            {
                return sub.GetString()!;
            }

            throw new InvalidOperationException("JWT missing or invalid 'sub' claim");
        }
    }

    public string Issuer
    {
        get
        {
            if (RootElement.TryGetProperty("iss", out var iss) && iss.ValueKind == JsonValueKind.String)
            {
                return iss.GetString()!;
            }

            throw new InvalidOperationException("JWT missing or invalid 'iss' claim");
        }
    }

    private List<string> ExtractAudience()
    {
        if (!RootElement.TryGetProperty("aud", out var aud))
        {
            throw new InvalidOperationException("JWT missing 'aud' claim");
        }

        return aud.ValueKind switch
        {
            JsonValueKind.String => new List<string> { aud.GetString()! },
            JsonValueKind.Array => aud.EnumerateArray()
                                .Where(e => e.ValueKind == JsonValueKind.String)
                                .Select(e => e.GetString()!)
                                .ToList(),
            _ => throw new InvalidOperationException("Unexpected type for 'aud' claim in JWT"),
        };
    }

    public IReadOnlyList<string> Audience => _audience.Value;

    // TODO: Should this be exposed as a JsonDocument, since that it in the stdlib?
    public string RawPayload => _payload;

    // Need to implement hashing.
    public Identity Identity => throw new NotImplementedException();
}
