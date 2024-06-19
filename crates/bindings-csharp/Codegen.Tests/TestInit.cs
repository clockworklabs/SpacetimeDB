namespace Codegen.Tests;

using System.Runtime.CompilerServices;

// Global Verify setup for all tests we might have.
static class TestInit
{
    [ModuleInitializer]
    public static void Initialize()
    {
        // Default diff order is weird and causes new lines to look like deleted and old as inserted.
        Environment.SetEnvironmentVariable("DiffEngine_TargetOnLeft", "true");
        // Store snapshots in a separate directory.
        UseProjectRelativeDirectory("snapshots");
        VerifySourceGenerators.Initialize();
        // Format code for more readable snapshots and to avoid diffs on whitespace changes.
        VerifierSettings.AddScrubber(
            "cs",
            sb =>
            {
                var unformattedCode = sb.ToString();
                sb.Clear();
                var result = CSharpier.CodeFormatter.Format(
                    unformattedCode,
                    new() { IncludeGenerated = true, EndOfLine = CSharpier.EndOfLine.LF }
                );
                if (result.CompilationErrors.Any())
                {
                    sb.AppendLine("// Generated code produced compilation errors:");
                    foreach (var diag in result.CompilationErrors)
                    {
                        sb.Append("// ").AppendLine(diag.ToString());
                    }
                    sb.AppendLine();
                }
                sb.Append(result.Code);
            },
            ScrubberLocation.Last
        );
    }
}
