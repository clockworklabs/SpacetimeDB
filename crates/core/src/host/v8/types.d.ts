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

    export type AlgebraicType = TypeRef | ProductType | ArrayType | PrimitiveType;
    export type TypeRef = Readonly<{
        type: 'ref';
        ref: number;
    }>;
    export type ProductType = Readonly<{
        type: 'product';
        elements: readonly ProductTypeElement[];
    }>;
    export type ProductTypeElement = Readonly<{
        name: string | null;
        algebraic_type: AlgebraicType;
    }>;
    export type ArrayType = Readonly<{
        type: 'array';
        elem_ty: AlgebraicType;
    }>;
    export type PrimitiveType = Readonly<
        | { type: 'string' }
        | { type: 'bool' }
        | { type: 'i8' }
        | { type: 'u8' }
        | { type: 'i16' }
        | { type: 'u16' }
        | { type: 'i32' }
        | { type: 'u32' }
        | { type: 'i64' }
        | { type: 'u64' }
        | { type: 'i128' }
        | { type: 'u128' }
        | { type: 'i256' }
        | { type: 'u256' }
        | { type: 'f32' }
        | { type: 'f64' }
    >;
}
