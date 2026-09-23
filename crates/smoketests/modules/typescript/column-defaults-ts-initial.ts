
import { schema, t, table } from "spacetimedb/server";

const defaultsTestTable = table(
  { name: "defaults_test_table", public: true },
  { id: t.u32() }
);

export default schema({ defaultsTestTable });
