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

    public static readonly ErrorDescriptor<AttributeData> EmptyIndexColumns =
        new(
            group,
            "Index attribute must specify Columns",
            _ => $"Index attribute doesn't specify columns.",
            attr => attr
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> InvalidTableVisibility =
        new(
            group,
            "Table row visibility must be public or internal",
            table => $"Table {table.Identifier} and its parent types must be public or internal.",
            table => table.Identifier
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

    public static readonly UnusedErrorDescriptor IncompatibleTableSchedule = new(group);

    public static readonly ErrorDescriptor<(
        ReducerKind kind,
        IEnumerable<string> fullNames
    )> DuplicateSpecialReducer =
        new(
            group,
            "Multiple reducers of the same kind",
            ctx =>
                $"Several reducers are assigned to the same lifecycle kind {ctx.kind}: {string.Join(", ", ctx.fullNames)}",
            ctx => Location.None
        );

    public static readonly ErrorDescriptor<(
        AttributeData attr,
        string message
    )> InvalidScheduledDeclaration =
        new(group, "Invalid scheduled table declaration", ctx => $"{ctx.message}", ctx => ctx.attr);

    public static readonly ErrorDescriptor<AttributeData> UnexpectedIndexColumns =
        new(
            group,
            "Index attribute on a field must not specify Columns",
            _ =>
                $"Index attribute on a field applies directly to that field, so it doesn't accept the Columns parameter.",
            attr => attr
        );

    public static readonly ErrorDescriptor<(
        AttributeData attr,
        string columnName,
        string typeName
    )> UnknownColumn =
        new(
            group,
            "Unknown column",
            ctx => $"Could not find the specified column {ctx.columnName} in {ctx.typeName}.",
            ctx => ctx.attr
        );

    public static readonly ErrorDescriptor<IFieldSymbol> ClientVisibilityNotFilter =
        new(
            group,
            "ClientVisibilityFilters must be Filters",
            field =>
                $"Field {field.Name} is marked as ClientVisibilityFilter but it has type {field.Type} which is not SpacetimeDB.Filter",
            field => field
        );

    public static readonly ErrorDescriptor<IFieldSymbol> ClientVisibilityNotPublicStaticReadonly =
        new(
            group,
            "ClientVisibilityFilters must be public static readonly",
            field =>
                $"Field {field.Name} is marked as [ClientVisibilityFilter] but it is not public static readonly",
            field => field
        );

    public static readonly ErrorDescriptor<IFieldSymbol> IncompatibleDefaultAttributesCombination =
        new(
            group,
            "Invalid Combination: AutoInc, Unique or PrimaryKey cannot have a Default value",
            field =>
                $"Field {field.Name} contains a default value and has a AutoInc, Unique or PrimaryKey attributes, which is not allowed.",
            field => field
        );

    public static readonly ErrorDescriptor<IFieldSymbol> InvalidDefaultValueType =
        new(
            group,
            "Invalid Default Value Type",
            field => $"Default value for field {field.Name} cannot be converted to provided type",
            field => field
        );

    public static readonly ErrorDescriptor<IFieldSymbol> InvalidDefaultValueFormat =
        new(
            group,
            "Invalid Default Value Format",
            field => $"Default value for field {field.Name} has invalid format for provided type ",
            field => field
        );
}
