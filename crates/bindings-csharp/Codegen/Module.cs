namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

record ColumnDeclaration : MemberDeclaration
{
    public readonly EquatableArray<(string? table, ColumnAttrs mask)> Attrs;
    public readonly bool IsEquatable;
    public readonly string FullTableName;

    public ColumnDeclaration(
        string tableName,
        string name,
        string type,
        string typeInfo,
        ColumnAttrs attrs,
        bool isEquatable
    )
        : base(name, type, typeInfo)
    {
        Attrs = new(ImmutableArray.Create((default(string), attrs)));
        IsEquatable = isEquatable;
        FullTableName = tableName;
    }

    public ColumnDeclaration(string tableName, IFieldSymbol field)
        : base(field)
    {
        FullTableName = tableName;

        Attrs = new(
            field
                .GetAttributes()
                .Select(a =>
                    (
                        table: a.NamedArguments.FirstOrDefault(a => a.Key == "Table").Value.Value
                            as string,
                        attr: a.AttributeClass?.ToString() switch
                        {
                            "SpacetimeDB.AutoIncAttribute" => ColumnAttrs.AutoInc,
                            "SpacetimeDB.PrimaryKeyAttribute" => ColumnAttrs.PrimaryKey,
                            "SpacetimeDB.UniqueAttribute" => ColumnAttrs.Unique,
                            "SpacetimeDB.IndexedAttribute" => ColumnAttrs.Indexed,
                            _ => ColumnAttrs.UnSet,
                        }
                    )
                )
                .Where(a => a.attr != ColumnAttrs.UnSet)
                .ToImmutableArray()
        );

        var type = field.Type;

        var isInteger = type.SpecialType switch
        {
            SpecialType.System_Byte
            or SpecialType.System_SByte
            or SpecialType.System_Int16
            or SpecialType.System_UInt16
            or SpecialType.System_Int32
            or SpecialType.System_UInt32
            or SpecialType.System_Int64
            or SpecialType.System_UInt64 => true,
            SpecialType.None => type.ToString()
                is "System.Int128"
                    or "System.UInt128"
                    or "SpacetimeDB.I128"
                    or "SpacetimeDB.U128"
                    or "SpacetimeDB.I256"
                    or "SpacetimeDB.U256",
            _ => false,
        };

        var attrs = Attrs.Aggregate(ColumnAttrs.UnSet, (xs, x) => xs | x.mask);

        if (attrs.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
        {
            throw new Exception(
                $"{type} {Name} is not valid for AutoInc or Identity as it's not an integer."
            );
        }

        IsEquatable =
            (
                isInteger
                || type.SpecialType switch
                {
                    SpecialType.System_String or SpecialType.System_Boolean => true,
                    SpecialType.None => type.ToString()
                        is "SpacetimeDB.Address"
                            or "SpacetimeDB.Identity",
                    _ => false,
                }
            )
            && type.NullableAnnotation != NullableAnnotation.Annotated;

        if (attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            throw new Exception(
                $"{type} {Name} is not valid for Identity, PrimaryKey or PrimaryKeyAuto as it's not an equatable primitive."
            );
        }
    }

    public ColumnAttrs GetAttrs(string tableName) =>
        Attrs
            .Where(x => x.table == null || x.table == tableName)
            .Aggregate(ColumnAttrs.UnSet, (xs, x) => xs | x.mask);

    // For the `TableDesc` constructor.
    public string GenerateColumnDef() =>
        $"new (nameof({Name}), BSATN.{Name}.GetAlgebraicType(registrar))";

    // For the `Filter` constructor.
    public string GenerateFilterEntry() =>
        $"new (nameof({Name}), (w, v) => BSATN.{Name}.Write(w, ({Type}) v!))";
}

record TableView
{
    public readonly string Name;
    public readonly bool IsPublic;
    public readonly string? Scheduled;

    public TableView(TableDeclaration table, AttributeData data)
    {
        Name =
            data.NamedArguments.FirstOrDefault(x => x.Key == "Name").Value.Value as string
            ?? table.ShortName;

        IsPublic = data.NamedArguments.Any(pair => pair is { Key: "Public", Value.Value: true });

        Scheduled = data
            .NamedArguments.Where(pair => pair.Key == "Scheduled")
            .Select(pair => (string?)pair.Value.Value)
            .SingleOrDefault();
    }
}

record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly Accessibility Visibility;
    public readonly string? Scheduled;
    public readonly EquatableArray<TableView> Views;

    private static ColumnDeclaration[] ScheduledColumns(string tableName) =>
        [
            new(
                tableName,
                "ScheduledId",
                "ulong",
                "SpacetimeDB.BSATN.U64",
                ColumnAttrs.PrimaryKeyAuto,
                true
            ),
            new(
                tableName,
                "ScheduledAt",
                "SpacetimeDB.ScheduleAt",
                "SpacetimeDB.ScheduleAt.BSATN",
                ColumnAttrs.UnSet,
                false
            ),
        ];

    public TableDeclaration(GeneratorAttributeSyntaxContext context)
        : base(context)
    {
        if (Kind is TypeKind.Sum)
        {
            throw new InvalidOperationException("Tagged enums cannot be tables.");
        }

        Visibility = context.TargetSymbol.DeclaredAccessibility;

        var container = context.TargetSymbol;
        while (container != null)
        {
            switch (Visibility)
            {
                case Accessibility.ProtectedAndInternal:
                case Accessibility.NotApplicable:
                case Accessibility.Internal:
                case Accessibility.Public:
                    break;
                default:
                    throw new Exception(
                        "Table row type visibility must be public or internal, including containing types."
                    );
            }
            ;

            container = container.ContainingType;
        }

        Views = new(context.Attributes.Select(a => new TableView(this, a)).ToImmutableArray());

        var schedules = Views.Select(t => t.Scheduled).Distinct();
        if (schedules.Count() != 1)
        {
            throw new Exception(
                "When using multiple [Table] attributes with schedule, all [Table] must have the same schedule."
            );
        }

        Scheduled = schedules.First();
        if (Scheduled != null)
        {
            // For scheduled tables, we append extra fields early in the pipeline,
            // both to the type itself and to the BSATN information, as if they
            // were part of the original declaration.
            Members = new(Members.Concat(ScheduledColumns(FullName)).ToImmutableArray());
        }
    }

    protected override ColumnDeclaration ConvertMember(IFieldSymbol field) => new(FullName, field);

    public IEnumerable<string> GenerateViewFilters(string viewName, string iTable)
    {
        foreach (
            var (f, i) in Members
                .Select((field, i) => (field, i))
                .Where(pair => pair.field.IsEquatable)
        )
        {
            var globalName = $"global::{FullName}";
            var colEqWhere = $"{iTable}.ColEq.Where({i}, {f.Name}, {globalName}.BSATN.{f.Name})";

            yield return $"""
                public IEnumerable<{globalName}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                    {colEqWhere}.Iter();
                """;

            if (f.GetAttrs(viewName).HasFlag(ColumnAttrs.Unique))
            {
                yield return $"""
                    public {globalName}? FindBy{f.Name}({f.Type} {f.Name}) =>
                        FilterBy{f.Name}({f.Name})
                        .Cast<{globalName}?>()
                        .SingleOrDefault();

                    public bool DeleteBy{f.Name}({f.Type} {f.Name}) =>
                        {colEqWhere}.Delete();

                    public bool UpdateBy{f.Name}({f.Type} {f.Name}, {globalName} @this) =>
                        {colEqWhere}.Update(@this);
                    """;
            }
        }
    }

    public record struct View(string viewName, string tableName, string view, string getter);

    public IEnumerable<View> GenerateViews()
    {
        foreach (var v in Views)
        {
            var autoIncFields = Members
                .Where(f => f.GetAttrs(v.Name).HasFlag(ColumnAttrs.AutoInc))
                .Select(f => f.Name);

            var globalName = $"global::{FullName}";
            var iTable = $"SpacetimeDB.Internal.ITableView<{v.Name}, {globalName}>";
            yield return new(
                v.Name,
                globalName,
                $$"""
            {{SyntaxFacts.GetText(Visibility)}} readonly struct {{v.Name}} : {{iTable}} {
                static {{globalName}} {{iTable}}.ReadGenFields(System.IO.BinaryReader reader, {{globalName}} row) {
                    {{string.Join(
                        "\n",
                        autoIncFields.Select(name =>
                            $$"""
                            if (row.{{name}} == default)
                            {
                                row.{{name}} = {{globalName}}.BSATN.{{name}}.Read(reader);
                            }
                            """
                        )
                    )}}
                    return row;
                }
                public IEnumerable<{{globalName}}> Iter() => {{iTable}}.Iter();
                public IEnumerable<{{globalName}}> Query(System.Linq.Expressions.Expression<Func<{{globalName}}, bool>> predicate) => {{iTable}}.Query(predicate);
                public {{globalName}} Insert({{globalName}} row) => {{iTable}}.Insert(row);
                {{string.Join("\n", GenerateViewFilters(v.Name, iTable))}}
            }
            """,
                $"{SyntaxFacts.GetText(Visibility)} Internal.TableHandles.{v.Name} {v.Name} => new();"
            );
        }
    }

    public override Scope.Extensions ToExtensions()
    {
        var extensions = base.ToExtensions();

        if (Scheduled is not null)
        {
            // For scheduled tables we're adding extra fields to the type source.
            extensions.Contents.Append(
                $$"""
                public ulong ScheduledId;
                public SpacetimeDB.ScheduleAt ScheduledAt;
                """
            );

            // When doing so, the compiler will warn about undefined ordering between partial declarations.
            // We don't care about ordering as we generate BSATN ourselves and don't use those structs in FFI,
            // so we can safely suppress the warning by saying "yes, we're okay with an auto/arbitrary layout".
            extensions.ExtraAttrs.Add(
                "[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Auto)]"
            );
        }

        var iTable = $"SpacetimeDB.Internal.ITable<{ShortName}>";

        // ITable inherits IStructuralReadWrite, so we can replace the base type instead of appending another one.
        extensions.BaseTypes.Clear();
        extensions.BaseTypes.Add(iTable);

        extensions.Contents.Append(
            $$"""
            static IEnumerable<SpacetimeDB.Internal.TableDesc> {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => [
            {{string.Join("\n", Views.Select(v => $$"""
            new (
                new (
                    TableName: nameof({{ShortName}}),
                    Columns: [
                        {{string.Join(",\n", Members.Select(m => m.GenerateColumnDef()))}}
                    ],
                    Indexes: [],
                    Constraints: [
                        {{string.Join(
                            ",\n",
                            Members
                            // Important: the position must be stored here, before filtering.
                            .Select((col, pos) => (col, pos, attr: col.GetAttrs(v.Name)))
                            .Where(tuple => tuple.attr != ColumnAttrs.UnSet)
                            .Select(tuple =>
                                $$"""
                                new (
                                    nameof({{ShortName}}),
                                    {{tuple.pos}},
                                    nameof({{tuple.col.Name}}),
                                    (SpacetimeDB.ColumnAttrs){{(int)tuple.attr}}
                                )
                                """
                            )
                        )}}
                    ],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "{{(v.IsPublic ? "public" : "private")}}",
                    Scheduled: {{(v.Scheduled is not null ? $"nameof({v.Scheduled})" : "null")}}
                ),
                (uint) ((SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)).Ref_
            ),
            """))}}
            ];

            static SpacetimeDB.Internal.Filter {{iTable}}.CreateFilter() => new([
                {{string.Join(",\n", Members.Select(f => f.GenerateFilterEntry()))}}
            ]);
            """
        );

        return extensions;
    }
}

record ReducerDeclaration
{
    public readonly string Name;
    public readonly string ExportName;
    public readonly string FullName;
    public readonly EquatableArray<MemberDeclaration> Args;
    public readonly Scope Scope;

    public ReducerDeclaration(GeneratorAttributeSyntaxContext context)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ReducerAttribute>();

        if (!method.ReturnsVoid)
        {
            throw new Exception($"Reducer {method} must return void");
        }
        if (
            method.Parameters.FirstOrDefault()?.Type is not INamedTypeSymbol namedType
            || namedType.Name != "ReducerContext"
        )
        {
            throw new Exception(
                $"Reducer {method} must have a first argument of type ReducerContext"
            );
        }

        Name = method.Name;
        ExportName = attr.Name ?? Name;
        FullName = SymbolToName(method);
        Args = new(
            method
                .Parameters.Skip(1)
                .Select(p => new MemberDeclaration(p.Name, p.Type))
                .ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }

    public KeyValuePair<string, string> GenerateClass()
    {
        var args = string.Join(
            ", ",
            Args.Select(a => $"{a.Name}.Read(reader)").Prepend("(SpacetimeDB.ReducerContext)ctx")
        );
        var class_ = $$"""
            class {{Name}}: SpacetimeDB.Internal.IReducer {
                {{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Args)}}

                public SpacetimeDB.Internal.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    "{{ExportName}}",
                    [{{MemberDeclaration.GenerateDefs(Args)}}]
                );

                public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx) {
                    {{FullName}}({{args}});
                }
            }
            """;

        return new(Name, class_);
    }

    public Scope.Extensions GenerateSchedule()
    {
        var extensions = new Scope.Extensions(Scope, FullName);

        extensions.Contents.Append(
            $$"""
            public static void VolatileNonatomicScheduleImmediate{{Name}}({{string.Join(
                ", ",
                Args.Select(a => $"{a.Type} {a.Name}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.TypeInfo}().Write(writer, {a.Name});")
                )}}
                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate("{{ExportName}}", stream);
            }
            """
        );

        return extensions;
    }
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(TableAttribute).FullName,
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) => new TableDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Table.Parse");

        tables
            .Select((t, ct) => t.ToExtensions())
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var reducers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(ReducerAttribute).FullName,
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) => new ReducerDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Reducer.Parse");

        reducers
            .Select((r, ct) => r.GenerateSchedule())
            .WithTrackingName("SpacetimeDB.Reducer.GenerateSchedule")
            .RegisterSourceOutputs(context);

        var addReducers = reducers
            .Select((r, ct) => r.GenerateClass())
            .WithTrackingName("SpacetimeDB.Reducer.GenerateClass")
            .Collect();

        var tableViews = tables
            .SelectMany((t, ct) => t.GenerateViews())
            .WithTrackingName("SpacetimeDB.Table.GenerateViews")
            .Collect();

        context.RegisterSourceOutput(
            tableViews.Combine(addReducers),
            (context, tuple) =>
            {
                // Sort tables and reducers by name to match Rust behaviour.
                // Not really important outside of testing, but for testing
                // it matters because we commit module-bindings
                // so they need to match 1:1 between different langs.
                var tableViews = tuple.Left.Sort((a, b) => a.viewName.CompareTo(b.viewName));
                var addReducers = tuple.Right.Sort((a, b) => a.Key.CompareTo(b.Key));
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (tableViews.IsEmpty && addReducers.IsEmpty)
                {
                    return;
                }
                context.AddSource(
                    "FFI.cs",
                    $$"""
                    // <auto-generated />
                    #nullable enable

                    using System.Diagnostics.CodeAnalysis;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;

                    namespace SpacetimeDB {
                        public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext {
                            public readonly Identity Sender;
                            public readonly Address? Address;
                            public readonly Random Random;
                            public readonly DateTimeOffset Time;

                            internal ReducerContext(Identity sender, Address? address, Random random, DateTimeOffset time) {
                                Sender = sender;
                                Address = address;
                                Random = random;
                                Time = time;
                            }
                        }

                        namespace Internal.TableHandles {
                            {{string.Join("\n", tableViews.Select(v => v.view))}}
                        }

                        public sealed class Local {
                            {{string.Join("\n", tableViews.Select(v => v.getter))}}
                        }
                    }

                    static class ModuleRegistration {
                        {{string.Join("\n", addReducers.Select(r => r.Value))}}

                    #if EXPERIMENTAL_WASM_AOT
                        // In AOT mode we're building a library.
                        // Main method won't be called automatically, so we need to export it as a preinit function.
                        [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
                    #else
                        // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                        [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(SpacetimeDB.Internal.Module))]
                    #endif
                        public static void Main() {
                            SpacetimeDB.Internal.Module.SetReducerContextConstructor((identity, address, random, time) => new SpacetimeDB.ReducerContext(identity, address, random, time));

                            {{string.Join(
                                "\n",
                                addReducers.Select(r =>
                                    $"SpacetimeDB.Internal.Module.RegisterReducer<{r.Key}>();"
                                )
                            )}}
                            {{string.Join(
                                "\n",
                                tableViews.Select(t => $"SpacetimeDB.Internal.Module.RegisterTable<{t.tableName}>();")
                            )}}
                        }

                    // Exports only work from the main assembly, so we need to generate forwarding methods.
                    #if EXPERIMENTAL_WASM_AOT
                        [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
                        public static void __describe_module__(SpacetimeDB.Internal.BytesSink d) => SpacetimeDB.Internal.Module.__describe_module__(d);

                        [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                        public static SpacetimeDB.Internal.Errno __call_reducer__(
                            uint id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong address_0,
                            ulong address_1,
                            SpacetimeDB.Internal.DateTimeOffsetRepr timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink error
                        ) => SpacetimeDB.Internal.Module.__call_reducer__(
                            id,
                            sender_0,
                            sender_1,
                            sender_2,
                            sender_3,
                            address_0,
                            address_1,
                            timestamp,
                            args,
                            error
                        );
                    #endif
                    }
                    """
                );
            }
        );
    }
}
