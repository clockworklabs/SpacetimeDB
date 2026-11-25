namespace System.Text;

using System.Collections.Generic;
using System.Globalization;

// netstandard2.0 lacks `Rune`. This provides the parts that the shared
// `CanonicalName.ToSnakeCase` (linked from the runtime) uses, with the same behaviour.
internal readonly record struct Rune(int Value)
{
    public static Rune ToLowerInvariant(Rune rune) => From(rune.ToString().ToLowerInvariant());

    public static Rune ToUpperInvariant(Rune rune) => From(rune.ToString().ToUpperInvariant());

    public static UnicodeCategory GetUnicodeCategory(Rune rune) =>
        CharUnicodeInfo.GetUnicodeCategory(rune.ToString(), 0);

    public override string ToString() => char.ConvertFromUtf32(Value);

    private static Rune From(string value) => new(char.ConvertToUtf32(value, 0));
}

internal static class RunePolyfillExtensions
{
    public static IEnumerable<Rune> EnumerateRunes(this string value)
    {
        for (var i = 0; i < value.Length; i++)
        {
            if (char.IsSurrogatePair(value, i))
            {
                yield return new(char.ConvertToUtf32(value, i));
                i++;
            }
            else
            {
                // Like `string.EnumerateRunes`, a lone surrogate becomes U+FFFD.
                yield return new(char.IsSurrogate(value[i]) ? 0xFFFD : value[i]);
            }
        }
    }

    public static bool Contains(this string value, char c) => value.IndexOf(c) >= 0;
}
