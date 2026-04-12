namespace SpacetimeDB
{
    internal enum WebSocketProtocolVersion
    {
        V2,
        V3,
    }

    internal static class WebSocketProtocols
    {
        internal const string V2 = "v2.bsatn.spacetimedb";
        internal const string V3 = "v3.bsatn.spacetimedb";

        internal static readonly string[] Preferred = new[] { V3, V2 };

        internal static WebSocketProtocolVersion Normalize(string? protocol)
        {
            // Treat an empty negotiated subprotocol as legacy v2 defensively.
            return protocol == V3 ? WebSocketProtocolVersion.V3 : WebSocketProtocolVersion.V2;
        }

#if UNITY_WEBGL && !UNITY_EDITOR
        internal static string SerializeOfferedProtocols(string[] protocols) => string.Join(",", protocols);
#endif
    }
}
