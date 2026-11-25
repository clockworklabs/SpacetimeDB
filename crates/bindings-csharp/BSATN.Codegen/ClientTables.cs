namespace SpacetimeDB.Codegen;

using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static ClientModule;
using static Utils;

internal static class ClientTables
{
    private static IFieldSymbol[] Fields(INamedTypeSymbol row) =>
        [
            .. row.GetAttributes()
                .Where(a =>
                    a.AttributeClass?.ToDisplayString()
                        is "SpacetimeDB.TypeAttribute"
                            or "SpacetimeDB.TableAttribute"
                )
                .Select(a =>
                    a.ApplicationSyntaxReference?.GetSyntax()
                        .FirstAncestorOrSelf<TypeDeclarationSyntax>()
                )
                .OfType<TypeDeclarationSyntax>()
                .Distinct()
                .SelectMany(syntax =>
                    SpacetimeDbFieldDiscovery.GetFieldsDeclaredInAnnotatedPartial(syntax, row)
                ),
        ];

    public static ClientTable Parse(INamedTypeSymbol row, AttributeData table)
    {
        var accessor = Option(table, "Accessor") ?? row.Name;
        var fields = Fields(row);
        var indexes = new List<ClientIndex>();
        var indexed = new HashSet<string>();
        bool Applies(AttributeData a) => Option(a, "Table") is not { } name || name == accessor;
        bool Has(IFieldSymbol field, string name) => Attributes(field, name).Any(Applies);
        var primary = fields.FirstOrDefault(f => Has(f, "PrimaryKey"));
        var unique = fields.Where(f => Has(f, "PrimaryKey") || Has(f, "Unique")).ToArray();

        void Index(string? name, IFieldSymbol[] columns, bool isUnique)
        {
            foreach (var field in columns)
                indexed.Add(field.Name);
            if (name != null && indexes.All(i => i.Name != name))
                indexes.Add(
                    new(
                        name,
                        columns,
                        isUnique
                            || (
                                columns.Length == 1
                                && unique.Contains(columns[0], SymbolEqualityComparer.Default)
                            )
                    )
                );
        }

        foreach (var index in Attributes(row, "Index.BTree").Where(Applies))
        {
            var names = index
                .NamedArguments.First(a => a.Key == "Columns")
                .Value.Values.Select(v => (string)v.Value!);
            var columns = names.Select(name => fields.Single(f => f.Name == name)).ToArray();
            Index(
                Option(index, "Accessor") ?? string.Join("", columns.Select(f => f.Name)),
                columns,
                false
            );
        }
        foreach (var index in Attributes(row, "ClientIndex").Where(Applies))
        {
            var args = index.ConstructorArguments;
            var columns = args[0]
                .Values.Select(v => fields.Single(f => f.Name == (string)v.Value!))
                .ToArray();
            Index(args[1].Value as string, columns, args[2].Value is true);
        }
        foreach (var field in fields)
        {
            foreach (var index in Attributes(field, "Index.BTree").Where(Applies))
                Index(Option(index, "Accessor") ?? field.Name, [field], false);
        }
        foreach (var field in unique)
            Index(field.Name, [field], true);

        return new(
            accessor,
            Option(table, "Name") ?? SnakeCase(accessor),
            row,
            fields,
            indexes,
            indexed,
            primary?.Name,
            Flag(table, "Event"),
            false
        );
    }

    public static ClientTable? ParseView(
        IMethodSymbol method,
        AttributeData view,
        string accessor,
        string wire
    )
    {
        var row = method.ReturnType switch
        {
            IArrayTypeSymbol array => array.ElementType as INamedTypeSymbol,
            INamedTypeSymbol { TypeArguments.Length: 1 } generic => generic.TypeArguments[0]
                as INamedTypeSymbol,
            INamedTypeSymbol named => named,
            _ => null,
        };
        if (row == null)
            return null;
        row = (INamedTypeSymbol)row.WithNullableAnnotation(NullableAnnotation.NotAnnotated);
        var fields = Fields(row);
        var key = Option(view, "PrimaryKey");
        var primary = fields.FirstOrDefault(f => f.Name == key);
        return new(
            accessor,
            wire,
            row,
            fields,
            primary == null ? [] : [new(primary.Name, [primary], true)],
            primary == null ? [] : [primary.Name],
            primary?.Name,
            false,
            method.Parameters.Length > 1
        );
    }

    private static string ColumnWireName(IFieldSymbol field) =>
        field
            .GetAttributes()
            .FirstOrDefault(a =>
                a.AttributeClass?.ToDisplayString()
                == "System.Runtime.Serialization.DataMemberAttribute"
            )
            is { } attr
        && Option(attr, "Name") is { } wire
            ? wire
            : SnakeCase(field.Name);

    internal static string TableName(ClientScope scope, ClientTable table) =>
        scope.Parent == null
            ? Literal(table.WireName)
            : $"RemoteTables.{table.Accessor}Handle.SqlName";

    public static string Emit(ClientScope scope, ClientTable table)
    {
        var output = new StringBuilder();
        var row = SymbolToName(table.Row);
        var handle = table.Accessor + "Handle";
        var baseName = table.Event ? "RemoteEventTableHandle" : "RemoteTableHandle";
        var context = $"global::{scope.Root.Namespace}.EventContext";
        var conn = $"global::{scope.Root.Namespace}.{scope.Root.ConnectionName}";
        output.AppendLine(
            $$"""
            public sealed partial class RemoteTables {
                public readonly {{handle}} {{table.Accessor}};
                public sealed class {{handle}} : global::SpacetimeDB.{{baseName}}<{{context}}, {{row}}> {
                    public override string RemoteTableName => {{Literal(
                scope.QualifiedWireName(table.WireName)
            )}};
            """
        );
        if (scope.Parent != null)
        {
            var segments = string.Join(", ", scope.WireSegments.Select(Literal));
            output.AppendLine(
                $$"""
                    internal static readonly global::SpacetimeDB.SqlTableName SqlName = new(new string[] { {{segments}} }, {{Literal(
                    table.WireName
                )}});
                    protected override global::SpacetimeDB.SqlTableName RemoteSqlTableName => SqlName;
                """
            );
        }
        foreach (var index in table.Indexes)
        {
            var columns = index.Fields;
            var keyType =
                columns.Length == 1
                    ? SymbolToName(columns[0].Type)
                    : "("
                        + string.Join(
                            ", ",
                            columns.Select(f =>
                                $"{SymbolToName(f.Type)} {EscapeIdentifier(f.Name)}"
                            )
                        )
                        + ")";
            var key =
                columns.Length == 1
                    ? "row." + EscapeIdentifier(columns[0].Name)
                    : "("
                        + string.Join(", ", columns.Select(f => "row." + EscapeIdentifier(f.Name)))
                        + ")";
            var indexName = index.Name + (index.Unique ? "UniqueIndex" : "Index");
            var indexBase = index.Unique ? "UniqueIndexBase" : "BTreeIndexBase";
            output.AppendLine(
                $$"""
                    public sealed class {{indexName}} : {{indexBase}}<{{keyType}}> {
                        protected override {{keyType}} GetKey({{row}} row) => {{key}};
                        public {{indexName}}({{handle}} table) : base(table) { }
                    }
                    public readonly {{indexName}} {{index.Name}};
                """
            );
        }
        if (table.PrimaryKey is { } pk)
            output.AppendLine(
                $"protected override object GetPrimaryKey({row} row) => row.{EscapeIdentifier(pk)};"
            );
        output.AppendLine($"internal {handle}({conn} conn) : base(conn) {{");
        foreach (var index in table.Indexes)
            output.AppendLine($"{index.Name} = new(this);");
        output.AppendLine("} } }");
        Columns("Col", table.Fields);
        Columns("IxCol", table.Fields.Where(f => table.IndexedColumns.Contains(f.Name)));
        return output.ToString();

        void Columns(string kind, IEnumerable<IFieldSymbol> fields)
        {
            var name = table.Accessor + kind + "s";
            var constructor = new StringBuilder();
            var nameType = scope.Parent == null ? "string" : "global::SpacetimeDB.SqlTableName";
            output.AppendLine($"public sealed class {name} {{");
            foreach (var field in fields)
            {
                var type =
                    field.Type is INamedTypeSymbol named
                    && named.OriginalDefinition.SpecialType == SpecialType.System_Nullable_T
                        ? named.TypeArguments[0]
                        : field.Type.WithNullableAnnotation(NullableAnnotation.NotAnnotated);
                var identifier = EscapeIdentifier(field.Name);
                output.AppendLine(
                    $"public global::SpacetimeDB.{kind}<{row}, {SymbolToName(type)}> {identifier} {{ get; }}"
                );
                constructor.AppendLine(
                    $"{identifier} = new(tableName, {Literal(ColumnWireName(field))});"
                );
            }
            output
                .AppendLine($"public {name}({nameType} tableName) {{")
                .Append(constructor)
                .AppendLine("} }");
        }
    }
}
