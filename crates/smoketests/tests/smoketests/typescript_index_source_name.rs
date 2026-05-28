use spacetimedb_smoketests::{random_string, require_local_server, require_pnpm, Smoketest};

const TYPESCRIPT_MODULE_WITHOUT_NEW_COLUMNS: &str = r#"import { schema, table, t } from "spacetimedb/server";

const targetableProperties_v1 = table(
  { name: "targetable_properties_v1", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    canonicalAddress: t.string(),
    googlePlaceId: t.string().index("btree"),
    googlePlacePhotoName: t.string().optional(),
  },
);

const spacetimedb = schema({
  targetableProperties_v1,
});
export default spacetimedb;

export const insert_targetable_property = spacetimedb.reducer(
  {
    canonicalAddress: t.string(),
    googlePlaceId: t.string(),
    googlePlacePhotoName: t.string(),
  },
  (ctx, { canonicalAddress, googlePlaceId, googlePlacePhotoName }) => {
    ctx.db.targetableProperties_v1.insert({
      id: 0n,
      canonicalAddress,
      googlePlaceId,
      googlePlacePhotoName,
    });
  },
);
"#;

const TYPESCRIPT_MODULE_WITH_NEW_COLUMNS: &str = r#"import { schema, table, t } from "spacetimedb/server";

const targetableProperties_v1 = table(
  { name: "targetable_properties_v1", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    canonicalAddress: t.string(),
    googlePlaceId: t.string().index("btree"),
    googlePlacePhotoName: t.string().optional(),
    latitude: t.number().optional().default(undefined),
    longitude: t.number().optional().default(undefined),
    sqft: t.number().optional().default(undefined),
    hasPhotos: t.bool().default(false).index(),
  },
);

const spacetimedb = schema({
  targetableProperties_v1,
});
export default spacetimedb;

export const touch_targetable_properties = spacetimedb.reducer(
  { googlePlaceId: t.string() },
  (ctx, { googlePlaceId }) => {
    let count = 0;
    for (const _row of ctx.db.targetableProperties_v1.googlePlaceId.filter(googlePlaceId)) {
      count += 1;
    }
    console.info(`matched ${count}`);
  },
);

export const touch_has_photos_index = spacetimedb.reducer(
  { hasPhotos: t.bool() },
  (ctx, { hasPhotos }) => {
    let count = 0;
    for (const _row of ctx.db.targetableProperties_v1.hasPhotos.filter(hasPhotos)) {
      count += 1;
    }
    console.info(`matched hasPhotos ${count}`);
  },
);
"#;

#[test]
fn test_typescript_add_optional_columns_to_indexed_table_then_call_reducer() {
    require_pnpm!();
    require_local_server!();

    let mut test = Smoketest::builder().autopublish(false).build();
    let module_name = format!("typescript-add-optional-columns-{}", random_string());

    let database_identity = test
        .publish_typescript_module_source(
            "typescript-add-optional-columns-v1",
            &module_name,
            TYPESCRIPT_MODULE_WITHOUT_NEW_COLUMNS,
        )
        .unwrap();

    test.call("insert_targetable_property", &["123 Main St", "place_123", "photo_123"])
        .unwrap();

    test.restart_server();

    test.publish_typescript_module_source_clear(
        "typescript-add-optional-columns-v2",
        &database_identity,
        TYPESCRIPT_MODULE_WITH_NEW_COLUMNS,
        false,
    )
    .unwrap();

    test.call("touch_targetable_properties", &["place_123"]).unwrap();
    test.call("touch_has_photos_index", &["false"]).unwrap();
}
