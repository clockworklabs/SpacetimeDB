namespace SpacetimeDB.Internal;

using System.Globalization;
using System.Text;

internal static class CanonicalName
{
    internal static string Convert(string source, CaseConversionPolicy policy)
    {
        if (policy == CaseConversionPolicy.None)
        {
            return source;
        }

        // Match the host's convert_case 0.6 default word boundaries. This runs only
        // when installing mounts or initializing an immediate-scheduling name cache.
        var elements = new List<string>();
        var iterator = StringInfo.GetTextElementEnumerator(source);
        while (iterator.MoveNext())
        {
            elements.Add(iterator.GetTextElement());
        }
        var result = new StringBuilder();
        var word = new StringBuilder();
        for (var i = 0; i <= elements.Count; i++)
        {
            var current = i < elements.Count ? elements[i] : "";
            var separator = current is "" or "_" or "-" or " ";
            var previous = i > 0 ? elements[i - 1] : "";
            var next = i + 1 < elements.Count ? elements[i + 1] : "";
            var boundary =
                separator
                || (IsLower(previous) && IsUpper(current))
                || (IsDigit(previous) && (IsLower(current) || IsUpper(current)))
                || ((IsLower(previous) || IsUpper(previous)) && IsDigit(current))
                || (IsUpper(previous) && IsUpper(current) && IsLower(next));
            if (boundary && word.Length > 0)
            {
                if (result.Length > 0)
                {
                    result.Append('_');
                }
                AppendLowercase(result, word.ToString());
                word.Clear();
            }
            if (!separator)
            {
                word.Append(current);
            }
        }
        return result.ToString();
    }

    private static bool IsDigit(string value) =>
        value.Length > 0 && value.All(c => c is >= '0' and <= '9');

    // These comparisons classify graphemes; a case-insensitive comparison would erase
    // the distinction needed by the word-boundary rules.
#pragma warning disable CA1862
    private static bool IsUpper(string value) =>
        (value.ToUpperInvariant() != value.ToLowerInvariant() || value.Contains('\u0130'))
        && value == value.ToUpperInvariant()
        && !value.EnumerateRunes().Any(HasUppercaseExpansion);

    private static bool IsLower(string value) =>
        (
            value.ToUpperInvariant() != value.ToLowerInvariant()
            || value.EnumerateRunes().Any(HasUppercaseExpansion)
        )
        && value == value.ToLowerInvariant()
        && !value.Contains('\u0130');
#pragma warning restore CA1862

    // Unicode unconditional full-uppercase expansions used by Rust's string casing.
    // .NET simple casing leaves some of these unchanged, affecting word boundaries.
    private static bool HasUppercaseExpansion(Rune rune) =>
        rune.Value
            is 0x00DF
                or 0x0149
                or 0x01F0
                or 0x0390
                or 0x03B0
                or 0x0587
                or >= 0x1E96
                and <= 0x1E9A
                or 0x1F50
                or 0x1F52
                or 0x1F54
                or 0x1F56
                or >= 0x1F80
                and <= 0x1FAF
                or >= 0x1FB2
                and <= 0x1FB4
                or >= 0x1FB6
                and <= 0x1FB7
                or 0x1FBC
                or >= 0x1FC2
                and <= 0x1FC4
                or >= 0x1FC6
                and <= 0x1FC7
                or 0x1FCC
                or >= 0x1FD2
                and <= 0x1FD3
                or >= 0x1FD6
                and <= 0x1FD7
                or >= 0x1FE2
                and <= 0x1FE4
                or >= 0x1FE6
                and <= 0x1FE7
                or >= 0x1FF2
                and <= 0x1FF4
                or >= 0x1FF6
                and <= 0x1FF7
                or 0x1FFC
                or >= 0xFB00
                and <= 0xFB06
                or >= 0xFB13
                and <= 0xFB17;

    private static void AppendLowercase(StringBuilder result, string word)
    {
        var runes = word.EnumerateRunes().ToArray();
        for (var i = 0; i < runes.Length; i++)
        {
            // Rust uses Unicode full lowercase mappings; .NET's invariant mapping
            // is simple and omits dotted-I expansion and contextual final sigma.
            if (runes[i].Value == 0x0130)
            {
                result.Append("i\u0307");
            }
            else if (
                runes[i].Value == 0x03A3
                && HasCasedRune(runes, i, -1)
                && !HasCasedRune(runes, i, 1)
            )
            {
                result.Append('\u03C2');
            }
            else
            {
                result.Append(Rune.ToLowerInvariant(runes[i]).ToString());
            }
        }
    }

    private static bool HasCasedRune(Rune[] runes, int index, int direction)
    {
        for (var i = index + direction; i >= 0 && i < runes.Length; i += direction)
        {
            var category = Rune.GetUnicodeCategory(runes[i]);
            if (
                category
                is UnicodeCategory.NonSpacingMark
                    or UnicodeCategory.EnclosingMark
                    or UnicodeCategory.Format
                    or UnicodeCategory.ModifierLetter
                    or UnicodeCategory.ModifierSymbol
            )
            {
                continue;
            }
            return Rune.ToUpperInvariant(runes[i]) != Rune.ToLowerInvariant(runes[i])
                || HasUppercaseExpansion(runes[i])
                || runes[i].Value == 0x0130
                || category == UnicodeCategory.TitlecaseLetter;
        }
        return false;
    }
}
