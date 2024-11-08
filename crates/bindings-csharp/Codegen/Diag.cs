namespace SpacetimeDB.Codegen;

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;

internal static class ErrorDescriptor
{
    private static readonly ErrorDescriptorGroup group = new("STDB", "SpacetimeDB");

    public static readonly ErrorDescriptor<MethodDeclarationSyntax> ReducerReturnType =
        new(
            group,
            "[SpacetimeDB.Reducer] methods must return void",
            method =>
                $"Reducer method {method.Identifier} returns {method.ReturnType} instead of void.",
            method => method.ReturnType
        );

    public static readonly ErrorDescriptor<IFieldSymbol> AutoIncNotInteger =
        new(
            group,
            "AutoInc fields must be of integer type",
            field =>
                $"Field {field.Name} is marked as AutoInc but it has a non-integer type {field.Type}.",
            field => field
        );

    public static readonly ErrorDescriptor<IFieldSymbol> UniqueNotEquatable =
        new(
            group,
            "Unique fields must be equatable",
            field =>
                $"Field {field.Name} is marked as Unique but it has a type {field.Type} which is not an equatable primitive.",
            field => field
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> EmptyIndexColumns =
        new(
            group,
            "Index attribute must specify index Columns.",
            table => $"Table {table.Identifier} has an Index.BTree attribute, but no columns.",
            table => table.BaseList!
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> InvalidTableVisibility =
        new(
            group,
            "Table row visibility must be public or internal, including container types.",
            table => $"Table {table.Identifier} and its parent types must be public or internal.",
            table => table.BaseList!
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> TableTaggedEnum =
        new(
            group,
            "Tables cannot be tagged enums",
            table => $"Table {table.Identifier} is a tagged enum, which is not allowed.",
            table => table.BaseList!
        );

    public static readonly ErrorDescriptor<(
        string kind,
        string exportName,
        IEnumerable<string> fullNames
    )> DuplicateExport =
        new(
            group,
            "Duplicate exports",
            ctx =>
                $"{ctx.kind} with the same export name {ctx.exportName} is registered in multiple places: {string.Join(", ", ctx.fullNames)}",
            ctx => Location.None
        );

    public static readonly ErrorDescriptor<MethodDeclarationSyntax> ReducerContextParam =
        new(
            group,
            "Reducers must have a first argument of type ReducerContext",
            method =>
                $"Reducer method {method.Identifier} does not have a ReducerContext parameter.",
            method => method.ParameterList
        );

    public static readonly ErrorDescriptor<(
        MethodDeclarationSyntax method,
        string prefix
    )> ReducerReservedPrefix =
        new(
            group,
            "Reducer method has a reserved name prefix",
            ctx =>
                $"Reducer method {ctx.method.Identifier} starts with '{ctx.prefix}', which is a reserved prefix.",
            ctx => ctx.method.Identifier
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> IncompatibleTableSchedule =
        new(
            group,
            "Incompatible `[Table(Schedule)]` attributes",
            table =>
                $"Schedule adds extra fields to the row type. Either all `[Table]` attributes should have a `Schedule`, or none of them.",
            table => table.SyntaxTree.GetLocation(table.AttributeLists.Span)
        );
}
