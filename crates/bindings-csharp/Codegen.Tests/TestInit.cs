namespace SpacetimeDB.Codegen.Tests;

using System.Runtime.CompilerServices;

// Global Verify setup for all tests we might have.
static class TestInit
{
    [ModuleInitializer]
    public static void Initialize()
    {
        VerifierSettings.OmitContentFromException();
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
                var result = CSharpier.CodeFormatter.Format(
                    unformattedCode,
                    new() { IncludeGenerated = true, EndOfLine = CSharpier.EndOfLine.LF }
                );
                sb.Append(result.Code);
                // Print errors in the end so that their line numbers are still meaningful.
                if (result.CompilationErrors.Any())
                {
                    sb.AppendLine();
                    sb.AppendLine("// Generated code produced compilation errors:");
                    foreach (var diag in result.CompilationErrors)
                    {
                        sb.Append("// ").AppendLine(diag.ToString());
                    }
                }
            },
            ScrubberLocation.Last
        );
    }
}
