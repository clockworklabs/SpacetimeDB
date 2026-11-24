using System.Text;

namespace SpacetimeDB.Codegen;

internal static class ClientConnectionEmitter
{
    private static readonly string Template = ReadTemplate();

    private static string ReadTemplate()
    {
        using var stream = typeof(ClientConnectionEmitter).Assembly.GetManifestResourceStream(
            "ClientConnectionTemplate"
        )!;
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    internal static string Emit(ClientScope root)
    {
        var queries = new StringBuilder();
        var dispatch = new StringBuilder();
        foreach (var scope in root.DescendantsAndSelf())
        {
            foreach (var table in scope.Tables.Where(t => !t.Parameterized))
            {
                queries.AppendLine(
                    $"            new QueryBuilder().From.{scope.MemberPath}{table.Accessor}().ToSql(),"
                );
            }

            foreach (
                var reducer in scope.Functions.Where(f => f.Kind == ClientFunctionKind.Reducer)
            )
            {
                dispatch.AppendLine(
                    $"                global::{scope.Namespace}.Reducer.{reducer.Name} args => Reducers.{scope.MemberPath}Invoke{reducer.Name}(eventContext, args),"
                );
            }
        }
        return Template
            .Replace("            /* all table queries */", queries.ToString().TrimEnd())
            .Replace("                /* reducer dispatch */", dispatch.ToString().TrimEnd());
    }
}
