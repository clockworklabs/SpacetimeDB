import {
    console_log,
    console_level_error,
    console_level_warn,
    console_level_info,
    console_level_debug,
    console_level_trace,
    console_level_panic,
    register_reducer,
    register_type,
} from 'spacetime:sys/v10.0';

function fmtLog(...data: unknown[]) {
    return data.join(' ');
}

const console = {
    __proto__: {},

    [Symbol.toStringTag]: 'console',

    assert: (condition = false, ...data: any) => {
        if (!condition) {
            console_log(console_level_error, fmtLog(...data));
        }
    },
    clear: () => {},
    debug: (...data: any) => {
        console_log(console_level_debug, fmtLog(...data));
    },
    error: (...data: any) => {
        console_log(console_level_error, fmtLog(...data));
    },
    info: (...data: any) => {
        console_log(console_level_info, fmtLog(...data));
    },
    log: (...data: any) => {
        console_log(console_level_info, fmtLog(...data));
    },
    table: (tabularData: unknown, properties: any) => {
        console_log(console_level_info, fmtLog(tabularData));
    },
    trace: (...data: any) => {
        console_log(console_level_trace, fmtLog(...data));
    },
    warn: (...data: any) => {
        console_log(console_level_warn, fmtLog(...data));
    },
    dir: (item: any, options: any) => {},
    dirxml: (...data: any) => {},

    // Counting
    count: (label = 'default') => {},
    countReset: (label = 'default') => {},

    // Grouping
    group: (...data: any) => {},
    groupCollapsed: (...data: any) => {},
    groupEnd: () => {},

    // Timing
    time: (label = 'default') => {},
    timeLog: (label = 'default', ...data: any) => {},
    timeEnd: (label = 'default') => {},
};
// @ts-ignore
globalThis.console = console;

const { freeze } = Object;

const stringType = Symbol('spacetimedb.type.string');
const boolType = Symbol('spacetimedb.type.bool');
const i8Type = Symbol('spacetimedb.type.i8');
const u8Type = Symbol('spacetimedb.type.u8');
const i16Type = Symbol('spacetimedb.type.i16');
const u16Type = Symbol('spacetimedb.type.u16');
const i32Type = Symbol('spacetimedb.type.i32');
const u32Type = Symbol('spacetimedb.type.u32');
const i64Type = Symbol('spacetimedb.type.i64');
const u64Type = Symbol('spacetimedb.type.u64');
const i128Type = Symbol('spacetimedb.type.i128');
const u128Type = Symbol('spacetimedb.type.u128');
const i256Type = Symbol('spacetimedb.type.i256');
const u256Type = Symbol('spacetimedb.type.u256');
const f32Type = Symbol('spacetimedb.type.f32');
const f64Type = Symbol('spacetimedb.type.f64');

export const type = freeze({
    string: stringType,
    bool: boolType,
    i8: i8Type,
    u8: u8Type,
    i16: i16Type,
    u16: u16Type,
    i32: i32Type,
    u32: u32Type,
    i64: i64Type,
    u64: u64Type,
    i128: i128Type,
    u128: u128Type,
    i256: i256Type,
    u256: u256Type,
    f32: f32Type,
    f64: f64Type,
    array<const Elem extends AlgebraicType>(elem: Elem) {
        return new ArrayType(elem);
    },
    product<const Map extends ProductMap>(map: Map) {
        return new ProductType(map);
    },
});

const toInternalType = Symbol('spacetimedb.toInternalType');

class ArrayType<Elem extends AlgebraicType> {
    #inner: import('spacetime:sys/v10.0').ArrayType;
    constructor(inner: Elem) {
        this.#inner = freeze({ type: 'array', elem_ty: convertType(inner) });
    }
    get [toInternalType]() {
        return this.#inner;
    }
}

type ProductMap = { [s: string]: AlgebraicType };
class ProductType<Map extends ProductMap> {
    #inner: import('spacetime:sys/v10.0').ProductType;
    constructor(map: Map) {
        const elements = freeze(
            Object.entries(map).map(([k, v]) => freeze({ name: k, algebraic_type: convertType(v) }))
        );
        this.#inner = freeze({ type: 'product', elements });
    }
    get [toInternalType]() {
        return this.#inner;
    }
}

class TypeRef<Type extends AlgebraicType> {
    #inner: import('spacetime:sys/v10.0').TypeRef;
    constructor(ref: number) {
        this.#inner = freeze({ type: 'ref', ref });
    }
    get [toInternalType]() {
        return this.#inner;
    }
}

const primitives = freeze({
    string: freeze({ type: 'string' }),
    bool: freeze({ type: 'bool' }),
    i8: freeze({ type: 'i8' }),
    u8: freeze({ type: 'u8' }),
    i16: freeze({ type: 'i16' }),
    u16: freeze({ type: 'u16' }),
    i32: freeze({ type: 'i32' }),
    u32: freeze({ type: 'u32' }),
    i64: freeze({ type: 'i64' }),
    u64: freeze({ type: 'u64' }),
    i128: freeze({ type: 'i128' }),
    u128: freeze({ type: 'u128' }),
    i256: freeze({ type: 'i256' }),
    u256: freeze({ type: 'u256' }),
    f32: freeze({ type: 'f32' }),
    f64: freeze({ type: 'f64' }),
});

function convertType(ty: AlgebraicType): import('spacetime:sys/v10.0').AlgebraicType {
    if (typeof ty === 'symbol') {
        switch (ty) {
            case type.string:
                return primitives.string;
            case type.bool:
                return primitives.bool;
            case type.i8:
                return primitives.i8;
            case type.u8:
                return primitives.u8;
            case type.i16:
                return primitives.i16;
            case type.u16:
                return primitives.u16;
            case type.i32:
                return primitives.i32;
            case type.u32:
                return primitives.u32;
            case type.i64:
                return primitives.i64;
            case type.u64:
                return primitives.u64;
            case type.i128:
                return primitives.i128;
            case type.u128:
                return primitives.u128;
            case type.i256:
                return primitives.i256;
            case type.u256:
                return primitives.u256;
            case type.f32:
                return primitives.f32;
            case type.f64:
                return primitives.f64;
        }
    } else if (ty != null) {
        const x = ty[toInternalType];
        if (x) return x;
    }
    throw new TypeError('Expected Spacetime type, got ' + ty);
}

type PrimitiveType = Extract<(typeof type)[keyof typeof type], symbol>;

type AlgebraicType = TypeRef<any> | ProductType<any> | ArrayType<any> | PrimitiveType;

export type I8 = number;
export type U8 = number;
export type I16 = number;
export type U16 = number;
export type I32 = number;
export type U32 = number;
export type I64 = bigint;
export type U64 = bigint;
export type I128 = bigint;
export type U128 = bigint;
export type I256 = bigint;
export type U256 = bigint;

type PrimitiveTypeToType<T extends PrimitiveType> = T extends typeof stringType
    ? string
    : T extends typeof boolType
    ? boolean
    : T extends typeof i8Type
    ? I8
    : T extends typeof u8Type
    ? U8
    : T extends typeof i16Type
    ? I16
    : T extends typeof u16Type
    ? U16
    : T extends typeof i32Type
    ? I32
    : T extends typeof u32Type
    ? U32
    : T extends typeof i64Type
    ? I64
    : T extends typeof u64Type
    ? U64
    : T extends typeof i128Type
    ? I128
    : T extends typeof u128Type
    ? U128
    : T extends typeof i256Type
    ? I256
    : T extends typeof u256Type
    ? U256
    : T extends typeof f32Type
    ? number
    : T extends typeof f64Type
    ? number
    : never;

type AlgebraicTypeToType<T extends AlgebraicType> = [T] extends [TypeRef<infer U>]
    ? AlgebraicTypeToType<U>
    : [T] extends [ProductType<infer U>]
    ? { [k in keyof U]: AlgebraicTypeToType<U[k]> }
    : [T] extends [ArrayType<infer U>]
    ? AlgebraicTypeToType<U>[]
    : [T] extends [PrimitiveType]
    ? PrimitiveTypeToType<T>
    : never;

type MakeArray<T> = T extends Array<any> ? T : never;

type ArgsToType<Args extends readonly AlgebraicType[]> = {
    [i in keyof Args]: AlgebraicTypeToType<Args[i]>;
};

export function registerReducer<const Args extends readonly AlgebraicType[]>(
    name: string,
    params: Args,
    func: (...args: ArgsToType<Args>) => void
) {
    if (typeof name !== 'string') {
        throw new TypeError('First argument to registerReducer must be string');
    }
    if (!Array.isArray(params)) {
        throw new TypeError('Second argument to registerReducer must be array');
    }
    const elements = freeze(
        params.map(ty => freeze({ name: null, algebraic_type: convertType(ty) }))
    );
    register_reducer(name, freeze({ type: 'product', elements }), func);
}

export function registerType<Type extends AlgebraicType>(name: string, type: Type): TypeRef<Type> {
    if (typeof name !== 'string') {
        throw new TypeError('First argument to registerType must be string');
    }
    const ref = register_type(name, convertType(type));
    return new TypeRef(ref);
}
