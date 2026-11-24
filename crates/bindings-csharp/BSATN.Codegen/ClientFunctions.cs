namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using static ClientModule;
using static Utils;

internal static class ClientFunctions
{
    // A Roslyn generator cannot run another generator on its own output. Construct the
    // same product model used by [Type] so argument and return wrappers receive the
    // same serializer, equality, and public BSATN type as a source-declared product.
    private sealed record Product(
        string Namespace,
        string Container,
        string Name,
        ImmutableArray<MemberDeclaration> Fields
    )
        : BaseTypeDeclaration<MemberDeclaration>(
            new Scope(SyntaxFactory.ClassDeclaration(Name)),
            Name,
            $"global::{Namespace}.{Container}.{Name}",
            new(Fields)
        )
    {
        protected override MemberDeclaration ConvertMember(
            int index,
            IFieldSymbol field,
            DiagReporter diag
        ) => new(field, diag);
    }

    private sealed record Parameter(
        string Name,
        string Field,
        string WireName,
        ITypeSymbol Type,
        TypeUse Use
    );

    private static Parameter Argument(IParameterSymbol p, DiagReporter diag)
    {
        var field = PascalCase(p.Name);
        var name = char.ToLowerInvariant(field[0]) + field[1..];
        return new(EscapeIdentifier(name), field, p.Name, p.Type, TypeUse.Parse(p, p.Type, diag));
    }

    private static string ProductClass(
        ClientScope scope,
        string container,
        string name,
        string bases,
        string member,
        IReadOnlyList<Parameter> parameters
    )
    {
        var model = new Product(
            scope.Namespace,
            container,
            name,
            parameters.Select(p => new MemberDeclaration(p.Field, p.Use)).ToImmutableArray()
        );
        var expansion = model.ToExtensions();
        var baseTypes = string.Join(
            ", ",
            expansion.BaseTypes.Prepend(bases).Where(b => b.Length > 0)
        );
        var b = new StringBuilder();
        b.AppendLine(
            $$"""
            public abstract partial class {{container}} {
                [global::SpacetimeDB.Type, global::System.Runtime.Serialization.DataContract]
                public sealed partial class {{name}} : {{baseTypes}} {
            """
        );
        foreach (var p in parameters)
        {
            b.AppendLine(
                $"[global::System.Runtime.Serialization.DataMember(Name = {Literal(p.WireName)})]"
            );
            b.AppendLine($"public {p.Use.Name} {EscapeIdentifier(p.Field)};");
        }
        if (parameters.Count != 0)
        {
            var sig = string.Join(
                ", ",
                parameters.Select(p => p.Use.Name + " " + EscapeIdentifier(p.Field))
            );
            b.AppendLine($"public {name}({sig}) {{");
            foreach (var p in parameters)
                b.AppendLine($"this.{EscapeIdentifier(p.Field)} = {EscapeIdentifier(p.Field)};");
            b.AppendLine("}").AppendLine($"public {name}() {{");
            foreach (var p in parameters)
            {
                if (ClientSchemaMembers.DefaultValue(p.Type) is { } value)
                {
                    b.AppendLine($"this.{EscapeIdentifier(p.Field)} = {value};");
                }
            }
            b.AppendLine("}");
        }
        return b.AppendLine(member)
            .Append(expansion.Contents)
            .AppendLine()
            .AppendLine("} }")
            .ToString();
    }

    public static string Emit(ClientScope scope, ClientFunction function, DiagReporter diag)
    {
        var parameters = function
            .Method.Parameters.Skip(1)
            .Select(p => Argument(p, diag))
            .ToArray();
        var signature = string.Join(", ", parameters.Select(p => $"{p.Use.Name} {p.Name}"));
        var args = string.Join(", ", parameters.Select(p => p.Name));
        var fields = string.Concat(parameters.Select(p => ", args." + EscapeIdentifier(p.Field)));
        var comma = parameters.Length == 0 ? "" : ", ";
        string Available(string name)
        {
            while (parameters.Any(p => p.Name == name))
                name += "_";
            return name;
        }

        var contextArg = Available("ctx");
        var callback = Available("callback");
        var name = function.Name;
        var ctx = $"global::{scope.Root.Namespace}.ReducerEventContext";
        var wire = Literal(scope.QualifiedWireName(function.WireName));

        if (function.Kind == ClientFunctionKind.Procedure)
        {
            var result = TypeUse.Parse(function.Method, function.Method.ReturnType, diag);
            var value = new Parameter(
                "value",
                "Value",
                "Value",
                function.Method.ReturnType,
                result
            );
            return $$"""
                public sealed partial class RemoteProcedures {
                    public void {{name}}({{signature}}{{comma}}global::SpacetimeDB.ProcedureCallback<{{result.Name}}> {{callback}}) {
                        this.conn.InternalCallProcedure<Procedure.{{name}}Args, Procedure.{{name}}>(
                            new Procedure.{{name}}Args({{args}}), (ctx, result) =>
                                {{callback}}(ctx, result.IsSuccess && result.Value != null
                                    ? global::SpacetimeDB.ProcedureCallbackResult<{{result.Name}}>.Success(result.Value.Value)
                                    : global::SpacetimeDB.ProcedureCallbackResult<{{result.Name}}>.Failure(result.Error!)));
                    }
                }
                {{ProductClass(scope, "Procedure", name, "", "", [value])}}
                {{ProductClass(scope, "Procedure", name + "Args", "Procedure, global::SpacetimeDB.IProcedureArgs",
                    $"string global::SpacetimeDB.IProcedureArgs.ProcedureName => {wire};", parameters)}}
                """;
        }

        var invoke = function.Invokable
            ? $"public void {name}({signature}) => this.conn.InternalCallReducer(new Reducer.{name}({args}));"
            : "";
        return $$"""
            public sealed partial class RemoteReducers {
                public delegate void {{name}}Handler({{ctx}} {{contextArg}}{{comma}}{{signature}});
                public event {{name}}Handler? On{{name}};
                {{invoke}}
                public bool Invoke{{name}}({{ctx}} ctx, Reducer.{{name}} args) {
                    if (On{{name}} == null) { Unhandled(ctx); return false; }
                    On{{name}}(ctx{{fields}});
                    return true;
                }
            }
            {{ProductClass(scope, "Reducer", name, $"global::{scope.Root.Namespace}.Reducer, global::SpacetimeDB.IReducerArgs",
                $"string global::SpacetimeDB.IReducerArgs.ReducerName => {wire};", parameters)}}
            """;
    }
}
