namespace SpacetimeDB.Codegen.Tests;

using System.Runtime.CompilerServices;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

// Global Verify setup for all tests we might have.
static class TestInit
{
    internal static (string Code, IEnumerable<Diagnostic> Errors) FormatCode(string code)
    {
        var header = "";
        // CSharpier reorders conditional global imports below ordinary imports.
        // Keep this already-formatted header intact and format the declarations only.
        if (code.Contains("global using "))
        {
            var root = CSharpSyntaxTree.ParseText(code).GetCompilationUnitRoot();
            var headerEnd = root.Usings.Last().FullSpan.End;
            header = code[..headerEnd];
            code = code[headerEnd..];
        }
        var result = CSharpier.Core.CSharp.CSharpFormatter.Format(
            code,
            new() { IncludeGenerated = true, EndOfLine = CSharpier.Core.EndOfLine.LF }
        );
#if NET10_0_OR_GREATER
        return (header + result.Code, result.ErrorDiagnostics);
#else
        return (header + result.Code, result.CompilationErrors);
#endif
    }

    // A custom Diagnostic converter that pretty-prints the error with the source code snippet and squiggly underline.
    // TODO: upstream this?
    class DiagConverter : WriteOnlyJsonConverter<Diagnostic>
    {
        public override void Write(VerifyJsonWriter writer, Diagnostic diag)
        {
            writer.WriteStartObject();
            // Pretty-print the error with the source code snippet.
            var loc = diag.Location;
            if (loc.SourceTree is { } source)
            {
                var comment = new StringBuilder().AppendLine();
                var lineSpan = loc.GetLineSpan();
                var lines = source.GetText().Lines;
                const int contextLines = 1;
                var startLine = Math.Max(lineSpan.StartLinePosition.Line - contextLines, 0);
                var endLine = Math.Min(
                    lineSpan.EndLinePosition.Line + contextLines,
                    lines.Count - 1
                );
                for (var lineIdx = startLine; lineIdx <= endLine; lineIdx++)
                {
                    var line = lines[lineIdx];
                    // print the source line
                    comment.AppendLine(line.ToString());
                    // print squiggly line highlighting the location
                    if (line.Span.Intersection(loc.SourceSpan) is { } intersection)
                    {
                        comment
                            .Append(' ', intersection.Start - line.Start)
                            .Append('^', intersection.Length)
                            .AppendLine();
                    }
                }
                writer.WriteComment(comment.ToString());
            }
            else
            {
                // Write location only when we don't have the source code snippet to make snapshots more stable.
                writer.WriteMember(diag, diag.Location, nameof(diag.Location));
            }
            writer.WriteMember(diag, diag.GetMessage(), "Message");
            writer.WriteMember(diag, diag.Severity, nameof(diag.Severity));
            writer.WriteMember(diag, diag.Descriptor, nameof(diag.Descriptor));
            writer.WriteEndObject();
        }
    }

    [ModuleInitializer]
    public static void Initialize()
    {
        if (Environment.GetEnvironmentVariable("CI") is null)
        {
            // Don't show the full diff in the console when running tests locally as Verify will open the diff tool automatically.
            VerifierSettings.OmitContentFromException();
        }
        // Default diff order is weird and causes new lines to look like deleted and old as inserted.
        Environment.SetEnvironmentVariable("DiffEngine_TargetOnLeft", "true");
        VerifySourceGenerators.Initialize();
        // Format code for more readable snapshots and to avoid diffs on whitespace changes.
        VerifierSettings.AddScrubber(
            "cs",
            (sb) =>
            {
                var unformattedCode = sb.ToString();
                sb.Clear();
                var result = FormatCode(unformattedCode);
                sb.Append(result.Code);
                // Print errors in the end so that their line numbers are still meaningful.
                if (result.Errors.Any())
                {
                    sb.AppendLine();
                    sb.AppendLine("// Generated code produced compilation errors:");
                    foreach (var diag in result.Errors)
                    {
                        sb.Append("// ").AppendLine(diag.ToString());
                    }
                }
            },
            ScrubberLocation.Last
        );
        VerifierSettings.AddExtraSettings(settings =>
        {
            settings.Converters.Insert(0, new DiagConverter());
        });
#if AUTO_VERIFY
        VerifierSettings.AutoVerify();
#endif
    }
}
