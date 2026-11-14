namespace SpacetimeDB.Internal;

internal static partial class CanonicalName
{
    internal static string Convert(string source, CaseConversionPolicy policy) =>
        policy == CaseConversionPolicy.None ? source : ToSnakeCase(source);
}
