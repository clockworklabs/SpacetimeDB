
import { schema, t, table } from "spacetimedb/server";

const defaultsTestTable = table(
  { name: "defaults_test_table", public: true },
  {
    id: t.u32(),
    bool_value: t.bool().default(true),
    i8_value: t.i8().default(-8),
    u8_value: t.u8().default(8),
    i16_value: t.i16().default(-16),
    u16_value: t.u16().default(16),
    i32_value: t.i32().default(-32),
    u32_value: t.u32().default(32),
    i64_value: t.i64().default(-64n),
    u64_value: t.u64().default(64n),
    f32_positive_value: t.f32().default(32.5),
    f32_negative_value: t.f32().default(-32.5),
    f64_positive_value: t.f64().default(64.25),
    f64_negative_value: t.f64().default(-64.25),
    string_value: t.string().default("default string"),
  }
);

export default schema({ defaultsTestTable });
