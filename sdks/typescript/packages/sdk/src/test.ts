import { AlgebraicType, ProductType, ProductTypeElement, SumTypeVariant } from "./algebraic_type";

type RawIdentifier = string;

type AlgebraicTypeRef = number;

type ColId = number;

type ColList = ColId[];

type RawIndexAlgorithm = 
  { tag: "btree", value: { columns: ColList } } |
  { tag: "hash", value: { columns: ColList } } |
  { tag: "direct", value: { column: ColId } };

type Typespace = {
  types: AlgebraicType[]; 
}

type RawIndexDefV9 = {
  name?: string,
  accessor_name?: RawIdentifier,
  algorithm: RawIndexAlgorithm,
}

type RawUniqueConstraintDataV9 = { columns: ColList };

type RawConstraintDataV9 =
  { tag: "unique", value: RawUniqueConstraintDataV9 };

type RawConstraintDefV9 = {
  name?: string,
  data: RawConstraintDataV9,
}

type RawSequenceDefV9 = {
  name?: RawIdentifier,
  column: ColId,
  start?: number,
  minValue?: number,
  maxValue?: number,
  increment: number
};

type TableType = "system" | "user";
type TableAccess = "public" | "private";

type RawScheduleDefV9 = {
  name?: RawIdentifier,
  reducerName: RawIdentifier,
  scheduledAtColumn: ColId,
};

type RawTableDefV9 = {
  name: RawIdentifier,
  productTypeRef: AlgebraicTypeRef,
  primaryKey: ColList,
  indexes: RawIndexDefV9[],
  constraints: RawConstraintDefV9[],
  sequences: RawSequenceDefV9[],
  schedule?: RawScheduleDefV9,
  tableType: TableType,
  tableAccess: TableAccess,
};

type RawReducerDefV9 = {
  name: RawIdentifier,
  params: ProductType,
  lifecycle?: "init" | "on_connect" | "on_disconnect",
}

type RawScopedTypeNameV9 = {
  name: RawIdentifier,
  scope: RawIdentifier[],
}

type RawTypeDefV9 = {
  name: RawScopedTypeNameV9,
  ty: AlgebraicTypeRef,
  customOrdering: boolean,
}

type RawMiscModuleExportV9 = never;

type RawSql = string;
type RawRowLevelSecurityDefV9 = {
  sql: RawSql
};

type RawModuleDef = { tag: "v8" } | { tag: "v9", value: RawModuleDefV9 };

type RawModuleDefV9 = {
  typespace: Typespace,
  tables: RawTableDefV9[],
  reducers: RawReducerDefV9[],
  types: RawTypeDefV9[],
  miscExports: RawMiscModuleExportV9[],
  rowLevelSecurity: RawRowLevelSecurityDefV9[],
}

const moduleDef: RawModuleDefV9 = {
  typespace: { types: [] },
  tables: [],
  reducers: [],
  types: [],
  miscExports: [],
  rowLevelSecurity: [],
}

/* ---------- column builder ---------- */
type Merge<M1, M2> = M1 & Omit<M2, keyof M1>;

export interface ColumnBuilder<
  JS,                       // the JavaScript/TypeScript value type
  M = {}                   // accumulated metadata: indexes, PKs, …
> {
  /** phantom – gives the column’s JS type to the compiler */
  readonly __type__: JS;
  readonly __spacetime_type__: AlgebraicType;

  index<N extends string = "btree">(name?: N):
    ColumnBuilder<JS, Merge<M, { index: N }>>;

  primary_key():
    ColumnBuilder<JS, Merge<M, { primaryKey: true }>>;

  auto_inc():
    ColumnBuilder<JS, Merge<M, { autoIncrement: true }>>;
}

/* minimal runtime implementation – chainable, metadata ignored */
function col<
  JS,
>(__spacetime_type__: AlgebraicType): ColumnBuilder<JS> {
  const c: any = { __spacetime_type__ };
  c.index = () => c;
  c.primary_key = () => c;
  c.auto_inc = () => c;
  return c;
}

/* ---------- primitive factories ---------- */
export const t = {
  /* ───── primitive scalars ───── */
  bool: (): ColumnBuilder<boolean> => col(AlgebraicType.createBoolType()),
  string: (): ColumnBuilder<string> => col(AlgebraicType.createStringType()),

  /* integers share JS = number but differ in Kind */
  i8: (): ColumnBuilder<number> => col(AlgebraicType.createI8Type()),
  u8: (): ColumnBuilder<number> => col(AlgebraicType.createU8Type()),
  i16: (): ColumnBuilder<number> => col(AlgebraicType.createI16Type()),
  u16: (): ColumnBuilder<number> => col(AlgebraicType.createU16Type()),
  i32: (): ColumnBuilder<number> => col(AlgebraicType.createI32Type()),
  u32: (): ColumnBuilder<number> => col(AlgebraicType.createU32Type()),
  i64: (): ColumnBuilder<number> => col(AlgebraicType.createI64Type()),
  u64: (): ColumnBuilder<number> => col(AlgebraicType.createU64Type()),
  i128: (): ColumnBuilder<number> => col(AlgebraicType.createI128Type()),
  u128: (): ColumnBuilder<number> => col(AlgebraicType.createU128Type()),
  i256: (): ColumnBuilder<number> => col(AlgebraicType.createI256Type()),
  u256: (): ColumnBuilder<number> => col(AlgebraicType.createU256Type()),

  f32: (): ColumnBuilder<number> => col(AlgebraicType.createF32Type()),
  f64: (): ColumnBuilder<number> => col(AlgebraicType.createF64Type()),

  number: (): ColumnBuilder<number> => col(AlgebraicType.createF64Type()),

  /* ───── structured builders ───── */
  object<Def extends Record<string, ColumnBuilder<any>>>(def: Def) {
    return {
      ...col(
        AlgebraicType.createProductType(
          Object.entries(def).map(([n, c]) =>
            new ProductTypeElement(n, c.__spacetime_type__))
        )
      ),
      __is_product_type__: true,
    } as ProductTypeColumnBuilder<Def>;
  },

  array<E extends ColumnBuilder<any>>(e: E): ColumnBuilder<Infer<E>[]> {
    return col<Infer<E>[]>(AlgebraicType.createArrayType(e.__spacetime_type__));
  },

  enum<
    V extends Record<string, ColumnBuilder<any>>,
  >(variants: V): ColumnBuilder<
      { [K in keyof V]: { tag: K } & { value: Infer<V[K]> } }[keyof V]
  > {
    return col<
      { [K in keyof V]: { tag: K } & { value: Infer<V[K]> } }[keyof V]
    >(
      AlgebraicType.createSumType(
        Object.entries(variants).map(
          ([n, c]) => new SumTypeVariant(n, c.__spacetime_type__)
        )
      )
    );
  },

} as const;

/* ─── brand marker ─────────────────────────── */
interface ProductTypeBrand {
  /** compile-time only – never set at runtime */
  readonly __is_product_type__: true;
}

/* ─── helper for ColumnBuilder that carries the brand ───────────────── */
export type ProductTypeColumnBuilder<
  Def extends Record<string, ColumnBuilder<any>>
> = ColumnBuilder<
  { [K in keyof Def]: ColumnType<Def[K]> }> & ProductTypeBrand;

/* ---------- utility: Infer<T> ---------- */
type ColumnType<C> =
  C extends ColumnBuilder<infer JS> ? JS : never;

export type Infer<S> =
  S extends ColumnBuilder<infer JS>
  ? JS
  : never;

/* ---------- table() ---------- */
export function table<
  Name extends string,
  Schema extends ProductTypeColumnBuilder<any>
>({ name, schema }: { name: Name, schema: Schema }) {
  moduleDef.tables.push({
    name,
    productTypeRef: moduleDef.typespace.types.length,
    primaryKey: [],
    indexes: [],
    constraints: [],
    sequences: [],
    schedule: undefined,
    tableType: "user",
    tableAccess: "private",
  });
  return {
    index<const IName extends string, I>(
      name: IName,
      _def: I
    ): undefined {
      return void 0;
    },
  };
}

/* ---------- reducer() ---------- */
type ParamsAsObject<P extends Record<string, ColumnBuilder<any>>> = {
  [K in keyof P]: Infer<P[K]>;
};

export function reducer<
  Name extends string,
  Params extends Record<string, ColumnBuilder<any>>,
  Ctx = unknown
>(
  name: Name,
  params: Params,
  fn: (ctx: Ctx, payload: ParamsAsObject<Params>) => void,
): undefined {
  /* compile‑time only */
  return void 0;
}

/* ---------- procedure() ---------- */
export function procedure<
  Name extends string,
  Params extends Record<string, ColumnBuilder<any>>,
  Ctx,
  R
>(
  name: Name,
  params: Params,
  fn: (ctx: Ctx, payload: ParamsAsObject<Params>) => Promise<R> | R,
): undefined {
  return void 0;
}

const point = t.object({
  x: t.f64(),
  y: t.f64(),
});
type Point = Infer<typeof point>;

const user = t.object({
  id: t.string(),
  name: t.string().index("btree"),
  email: t.string(),
  age: t.number(),
});
type User = Infer<typeof user>;

table("user", user);
table("logged_out_user", user);

const player = t.object({
  id: t.u32().primary_key().auto_inc(),
  name: t.string().index("btree"),
  score: t.number(),
  level: t.number(),
  foo: t.number(),
  bar: t.object({
    x: t.f64(),
    y: t.f64(),
  }),
  baz: t.enum({
    Foo: t.f64(),
    Bar: t.f64(),
    Baz: t.string(),
  }),
});

table("player", player).index("foobar", {
  btree: {
    columns: ["name", "score"],
  }
});

reducer("move_player", { user, foo: point, player }, (ctx, { user, foo: Point, player }) => {
  if (player.baz.tag === "Foo") {
    player.baz.value += 1;
  } else if (player.baz.tag === "Bar") {
    player.baz.value += 2;
  } else if (player.baz.tag === "Baz") {
    player.baz.value += "!";
  }
});

procedure("get_user", { user }, async (ctx, { user }) => {
  // return ctx.db.query("SELECT * FROM user WHERE id = ?", [user.id]);
});

//////


// const t = AlgebraicType;

// function spacetimeType(foo: AlgebraicType) {

// }

// const Foo = spacetimeType(t.createProductType([
//     new ProductTypeElement("x", t.createSumType([
//         new SumTypeVariant("Bar1", t.createF64Type()),
//         new SumTypeVariant("Bar2", t.createF64Type()),
//     ])),
//     new ProductTypeElement("y", t.createSumType([
//         new SumTypeVariant("Foo1", t.createF64Type()),
//         new SumTypeVariant("Foo2", t.createF64Type()),
//     ])),
// ]));