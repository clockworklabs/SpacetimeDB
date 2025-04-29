declare module 'spacetime:sys/v10.0' {
    export const console_level_error: unique symbol;
    export const console_level_warn: unique symbol;
    export const console_level_info: unique symbol;
    export const console_level_debug: unique symbol;
    export const console_level_trace: unique symbol;
    export const console_level_panic: unique symbol;
    type ConsoleLevel =
        | typeof console_level_error
        | typeof console_level_warn
        | typeof console_level_info
        | typeof console_level_debug
        | typeof console_level_trace
        | typeof console_level_panic;

    export function console_log(level: ConsoleLevel, msg: string): void;

    export function register_reducer(name: string, product_type: ProductType, func: Function): void;

    export function register_type(name: string, type: AlgebraicType): number;

    type Variant<Tag extends string, Value> = Readonly<{ tag: Tag; value: Value }>;

    export type option<T> = Readonly<{ some: T }> | null;

    export type AlgebraicType =
        | Variant<'Ref', number>
        | Variant<'Product', ProductType>
        | ArrayVariant
        | PrimitiveType;
    export type ProductType = Readonly<{
        elements: readonly ProductTypeElement[];
    }>;
    export type ProductTypeElement = Readonly<{
        name?: option<string>;
        algebraic_type: AlgebraicType;
    }>;
    type ArrayVariant = Readonly<{ tag: 'Array'; value: AlgebraicType }>;
    export type Unit = Readonly<{}>;
    export type PrimitiveType =
        | Variant<'String', Unit>
        | Variant<'Bool', Unit>
        | Variant<'I8', Unit>
        | Variant<'U8', Unit>
        | Variant<'I16', Unit>
        | Variant<'U16', Unit>
        | Variant<'I32', Unit>
        | Variant<'U32', Unit>
        | Variant<'I64', Unit>
        | Variant<'U64', Unit>
        | Variant<'I128', Unit>
        | Variant<'U128', Unit>
        | Variant<'I256', Unit>
        | Variant<'U256', Unit>
        | Variant<'F32', Unit>
        | Variant<'F64', Unit>;
}
