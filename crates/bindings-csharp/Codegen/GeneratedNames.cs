namespace SpacetimeDB.Codegen;

using Microsoft.CodeAnalysis.CSharp;

internal delegate void GeneratedNameCollisionReporter(
    string scope,
    string name,
    string firstContributor,
    string secondContributor
);

// Compare symbols in their emitted C# scope; @ escaping does not distinguish names.
sealed class GeneratedNames(GeneratedNameCollisionReporter report)
{
    private readonly Dictionary<(string Scope, string Name), string> declarations = [];

    internal void Add(string scope, string identifier, string contributor)
    {
        var name = SyntaxFactory.ParseToken(identifier).ValueText;
        var key = (scope, name);
        if (declarations.TryGetValue(key, out var previous))
        {
            report(scope, name, previous, contributor);
        }
        else
        {
            declarations.Add(key, contributor);
        }
    }
}
