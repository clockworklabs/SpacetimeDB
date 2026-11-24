namespace SpacetimeDB.Codegen;

using System.Text;
using static Utils;

internal static class ClientContainers
{
    public static string Emit(ClientScope scope)
    {
        var output = new StringBuilder();
        var root = $"global::{scope.Root.Namespace}";
        var connection = $"{root}.{scope.ConnectionName}";

        void Children(string container)
        {
            foreach (var child in scope.Children)
                output.AppendLine(
                    $"public global::{child.Namespace}.{container} {child.Accessor} {{ get; }}"
                        + (container == "From" ? " = new();" : "")
                );
        }

        foreach (var container in new[] { "RemoteReducers", "RemoteProcedures" })
        {
            output.AppendLine(
                $"public sealed partial class {container} : global::SpacetimeDB.RemoteBase {{"
            );
            Children(container);
            if (container == "RemoteReducers")
            {
                output.AppendLine(
                    $"internal event Action<{root}.ReducerEventContext, Exception>? InternalOnUnhandledReducerError;"
                );
                output.AppendLine(
                    $$"""
                    private void Unhandled({{root}}.ReducerEventContext ctx) {
                        if (InternalOnUnhandledReducerError == null) return;
                        switch (ctx.Event.Status) {
                            case global::SpacetimeDB.Status.Failed(var reason): InternalOnUnhandledReducerError(ctx, new Exception(reason)); break;
                            case global::SpacetimeDB.Status.OutOfEnergy _: InternalOnUnhandledReducerError(ctx, new Exception("out of energy")); break;
                            case global::SpacetimeDB.Status.UnknownResult _: InternalOnUnhandledReducerError(ctx, new global::SpacetimeDB.UnknownResultException()); break;
                        }
                    }
                    """
                );
            }
            output.AppendLine($"internal {container}({connection} conn) : base(conn) {{");
            foreach (var child in scope.Children)
            {
                output.AppendLine($"{child.Accessor} = new(conn);");
                if (container == "RemoteReducers")
                    output.AppendLine(
                        $"{child.Accessor}.InternalOnUnhandledReducerError += (ctx, error) => InternalOnUnhandledReducerError?.Invoke(ctx, error);"
                    );
            }
            output.AppendLine("} }");
        }

        var isRoot = scope.Parent == null;
        output.AppendLine(
            "public sealed partial class RemoteTables"
                + (isRoot ? " : global::SpacetimeDB.RemoteTablesBase" : "")
                + " {"
        );
        Children("RemoteTables");
        output.AppendLine(
            isRoot
                ? $"public RemoteTables({connection} conn) {{"
                : $"internal RemoteTables({connection} conn, Action<global::SpacetimeDB.IRemoteTableHandle> register) {{"
        );
        var register = isRoot ? "AddTable" : "register";
        foreach (var table in scope.Tables)
            output.AppendLine($"{register}({table.Accessor} = new(conn));");
        foreach (var child in scope.Children)
            output.AppendLine($"{child.Accessor} = new(conn, {register});");
        output.AppendLine("} }");
        output.AppendLine("public sealed class From {");
        Children("From");
        foreach (var table in scope.Tables)
        {
            var name = table.Accessor;
            var sql = ClientTables.TableName(scope, table);
            output.AppendLine(
                $"public global::SpacetimeDB.Table<{SymbolToName(table.Row)}, {name}Cols, {name}IxCols> {name}() => new({sql}, new {name}Cols({sql}), new {name}IxCols({sql}));"
            );
        }
        var access = isRoot && scope.Children.Count > 0 ? "private protected" : "private";
        output
            .AppendLine("}")
            .AppendLine($"public abstract partial class Reducer {{ {access} Reducer() {{ }} }}")
            .AppendLine("public abstract partial class Procedure { private Procedure() { } }");
        return output.ToString();
    }
}
