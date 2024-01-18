namespace SpacetimeDB.Codegen;

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

[System.Flags]
enum ColumnAttrs : byte
{
    UnSet = 0b0000,
    Indexed = 0b0001,
    AutoInc = 0b0010,
    Unique = Indexed | 0b0100,
    Identity = Unique | AutoInc,
    PrimaryKey = Unique | 0b1000,
    PrimaryKeyAuto = PrimaryKey | AutoInc,
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context
            .SyntaxProvider
            .ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.TableAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                {
                    var table = (TypeDeclarationSyntax)context.TargetNode;

                    var resolvedTable =
                        (ITypeSymbol?)context.SemanticModel.GetDeclaredSymbol(table)
                        ?? throw new System.Exception("Could not resolve table");

                    var fields = resolvedTable
                        .GetMembers()
                        .OfType<IFieldSymbol>()
                        .Where(f => !f.IsStatic)
                        .Select(f =>
                        {
                            var indexKind = f.GetAttributes()
                                .Where(
                                    a =>
                                        a.AttributeClass?.ToDisplayString()
                                        == "SpacetimeDB.ColumnAttribute"
                                )
                                .Select(a => (ColumnAttrs)a.ConstructorArguments[0].Value!)
                                .SingleOrDefault();

                            if (indexKind.HasFlag(ColumnAttrs.AutoInc))
                            {
                                var isValidForAutoInc = f.Type.SpecialType switch
                                {
                                    SpecialType.System_Byte
                                    or SpecialType.System_SByte
                                    or SpecialType.System_Int16
                                    or SpecialType.System_UInt16
                                    or SpecialType.System_Int32
                                    or SpecialType.System_UInt32
                                    or SpecialType.System_Int64
                                    or SpecialType.System_UInt64
                                        => true,
                                    SpecialType.None
                                        => f.Type.ToString() switch
                                        {
                                            "System.Int128" or "System.UInt128" => true,
                                            _ => false
                                        },
                                    _ => false
                                };

                                if (!isValidForAutoInc)
                                {
                                    throw new System.Exception(
                                        $"Type {f.Type} is not valid for AutoInc or Identity as it's not an integer."
                                    );
                                }
                            }

                            return (
                                Name: f.Name,
                                Type: SymbolToName(f.Type),
                                TypeInfo: GetTypeInfo(f.Type),
                                IndexKind: indexKind
                            );
                        })
                        .ToArray();

                    return new
                    {
                        Scope = new Scope(table),
                        Name = table.Identifier.Text,
                        FullName = SymbolToName(context.SemanticModel.GetDeclaredSymbol(table)!),
                        Fields = fields,
                    };
                }
            );

        tables
            .Select(
                (t, ct) =>
                {
                    var autoIncFields = t.Fields
                        .Where(f => f.IndexKind.HasFlag(ColumnAttrs.AutoInc))
                        .Select(f => f.Name);

                    var extensions =
                        $@"
                            private static readonly Lazy<SpacetimeDB.RawBindings.TableId> tableId = new (() => SpacetimeDB.Runtime.GetTableId(nameof({t.Name})));

                            public static IEnumerable<{t.Name}> Iter() =>
                                new SpacetimeDB.Runtime.RawTableIter(tableId.Value)
                                .SelectMany(GetSatsTypeInfo().ReadBytes);

                            private static readonly Lazy<KeyValuePair<string, SpacetimeDB.SATS.TypeInfo<object?>>[]> fieldTypeInfos = new (() => new KeyValuePair<string, SpacetimeDB.SATS.TypeInfo<object?>>[] {{
                                {string.Join("\n", t.Fields.Select(f => $"new (nameof({f.Name}), {f.TypeInfo}.EraseType()),"))}
                            }});

                            public static SpacetimeDB.Module.TableDesc MakeTableDesc() => new (
                                new (
                                    nameof({t.Name}),
                                    new SpacetimeDB.Module.ColumnDefWithAttrs[] {{ {string.Join(",", t.Fields.Select(f => $@"
                                        new (
                                            new SpacetimeDB.Module.ColumnDef(nameof({f.Name}), {f.TypeInfo}.AlgebraicType),
                                            SpacetimeDB.Module.ColumnAttrs.{f.IndexKind}
                                        )
                                    "))} }}
                                ),
                                {t.Name}.GetSatsTypeInfo().AlgebraicType.TypeRef
                            );

                            public static IEnumerable<{t.Name}> Query(System.Linq.Expressions.Expression<Func<{t.Name}, bool>> filter) =>
                                new SpacetimeDB.Runtime.RawTableIter(tableId.Value, SpacetimeDB.Filter.Filter.Compile<{t.Name}>(fieldTypeInfos.Value, filter))
                                .SelectMany(GetSatsTypeInfo().ReadBytes);

                            public void Insert() {{
                                var typeInfo = GetSatsTypeInfo();
                                var bytes = typeInfo.ToBytes(this);
                                SpacetimeDB.Runtime.Insert(tableId.Value, bytes);
                                // bytes should contain modified value now with autoinc fields updated
                                {(autoIncFields.Any() ? $@"
                                    var newInstance = typeInfo.ReadBytes(bytes).SingleOrDefault();

                                    {string.Join("\n", autoIncFields.Select(f => $"this.{f} = newInstance.{f};"))}
                                " : "")}
                            }}
                        ";

                    foreach (
                        var (f, index) in t.Fields.Select(
                            (f, i) => (f, $"new SpacetimeDB.RawBindings.ColId({i})")
                        )
                    )
                    {
                        if (f.IndexKind.HasFlag(ColumnAttrs.Unique))
                        {
                            extensions +=
                                $@"
                                    public static {t.Name}? FindBy{f.Name}({f.Type} {f.Name}) =>
                                        GetSatsTypeInfo().ReadBytes(
                                            SpacetimeDB.Runtime.IterByColEq(tableId.Value, {index}, {f.TypeInfo}.ToBytes({f.Name}))
                                        )
                                        .Cast<{t.Name}?>()
                                        .SingleOrDefault();

                                    public static bool DeleteBy{f.Name}({f.Type} {f.Name}) =>
                                        SpacetimeDB.Runtime.DeleteByColEq(tableId.Value, {index}, {f.TypeInfo}.ToBytes({f.Name})) > 0;

                                    public static bool UpdateBy{f.Name}({f.Type} {f.Name}, {t.Name} value) =>
                                        SpacetimeDB.Runtime.UpdateByColEq(tableId.Value, {index}, {f.TypeInfo}.ToBytes({f.Name}), GetSatsTypeInfo().ToBytes(value));
                                ";
                        }

                        extensions +=
                            $@"
                                public static IEnumerable<{t.Name}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                                    GetSatsTypeInfo().ReadBytes(
                                        SpacetimeDB.Runtime.IterByColEq(tableId.Value, {index}, {f.TypeInfo}.ToBytes({f.Name}))
                                    );
                            ";
                    }

                    return new KeyValuePair<string, string>(
                        t.FullName,
                        t.Scope.GenerateExtensions(extensions)
                    );
                }
            )
            .RegisterSourceOutputs(context);

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var reducers = context
            .SyntaxProvider
            .ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.ReducerAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                {
                    var method = (IMethodSymbol)
                        context.SemanticModel.GetDeclaredSymbol(context.TargetNode)!;

                    if (!method.ReturnsVoid)
                    {
                        throw new System.Exception($"Reducer {method} must return void");
                    }

                    var exportName = (string?)
                        context
                            .Attributes
                            .SingleOrDefault()
                            ?.ConstructorArguments
                            .SingleOrDefault()
                            .Value;

                    return new
                    {
                        Name = method.Name,
                        ExportName = exportName ?? method.Name,
                        FullName = SymbolToName(method),
                        Args = method
                            .Parameters
                            .Select(
                                p =>
                                    (
                                        p.Name,
                                        p.Type,
                                        IsDbEvent: p.Type.ToString()
                                            == "SpacetimeDB.Runtime.DbEventArgs"
                                    )
                            )
                            .ToArray(),
                        Scope = new Scope((TypeDeclarationSyntax)context.TargetNode.Parent!)
                    };
                }
            );

        var addReducers = reducers
            .Select(
                (r, ct) =>
                    (
                        r.Name,
                        Class: $@"
                            class {r.Name}: IReducer {{
                                {string.Join("\n", r.Args.Where(a => !a.IsDbEvent).Select(a => $"SpacetimeDB.SATS.TypeInfo<{a.Type}> {a.Name} = {GetTypeInfo(a.Type)};"))}

                                SpacetimeDB.Module.ReducerDef IReducer.MakeReducerDef() {{
                                    return new (
                                        ""{r.ExportName}""
                                        {string.Join("", r.Args.Where(a => !a.IsDbEvent).Select(a => $",\nnew SpacetimeDB.SATS.ProductTypeElement(nameof({a.Name}), {a.Name}.AlgebraicType)"))}
                                    );
                                }}

                                void IReducer.Invoke(BinaryReader reader, SpacetimeDB.Runtime.DbEventArgs dbEvent) {{
                                    {r.FullName}({string.Join(", ", r.Args.Select(a => a.IsDbEvent ? "dbEvent" : $"{a.Name}.Read(reader)"))});
                                }}
                            }}
                        "
                    )
            )
            .Collect();

        context.RegisterSourceOutput(
            tableNames.Combine(addReducers),
            (context, tuple) =>
            {
                // Sort tables and reducers by name to match Rust behaviour.
                // Not really important outside of testing, but for testing
                // it matters because we commit module-bindings
                // so they need to match 1:1 between different langs.
                var tableNames = tuple.Left.Sort();
                var addReducers = tuple.Right.Sort((a, b) => a.Name.CompareTo(b.Name));
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (tableNames.IsEmpty && addReducers.IsEmpty)
                    return;
                context.AddSource(
                    "FFI.cs",
                    $@"
            // <auto-generated />
            #nullable enable

            using SpacetimeDB.Module;
            using System.Runtime.CompilerServices;
            using static SpacetimeDB.Runtime;
            using System.Diagnostics.CodeAnalysis;

            static class ModuleRegistration {{
                {string.Join("\n", addReducers.Select(r => r.Class))}

#pragma warning disable CA2255
                // [ModuleInitializer] - doesn't work because assemblies are loaded lazily;
                // might make use of it later down the line, but for now assume there is only one
                // module so we can use `Main` instead.

                // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                [DynamicDependency(DynamicallyAccessedMemberTypes.All, typeof(SpacetimeDB.Module.FFI))]
                public static void Main() {{
                    {string.Join("\n", addReducers.Select(r => $"FFI.RegisterReducer(new {r.Name}());"))}
                    {string.Join("\n", tableNames.Select(t => $"FFI.RegisterTable({t}.MakeTableDesc());"))}
                }}
#pragma warning restore CA2255
            }}
            "
                );
            }
        );

        reducers
            .Select(
                (r, ct) =>
                    new KeyValuePair<string, string>(
                        r.FullName,
                        r.Scope.GenerateExtensions(
                            $@"
                            public static SpacetimeDB.Runtime.ScheduleToken Schedule{r.Name}(DateTimeOffset time{string.Join("", r.Args.Where(a => !a.IsDbEvent).Select(a => $", {a.Type} {a.Name}"))}) {{
                                using var stream = new MemoryStream();
                                using var writer = new BinaryWriter(stream);
                                {string.Join("\n", r.Args.Where(a => !a.IsDbEvent).Select(a => $"{GetTypeInfo(a.Type)}.Write(writer, {a.Name});"))}
                                return new(nameof({r.Name}), stream.ToArray(), time);
                            }}
                        "
                        )
                    )
            )
            .RegisterSourceOutputs(context);
    }
}
