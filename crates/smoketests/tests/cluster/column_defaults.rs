use spacetimedb_smoketests::{allow_dotnet, random_string, Smoketest};

const EXPECTED_DEFAULTS: &[(&str, &str)] = &[
    ("bool_value", "true"),
    ("u8_value", "8"),
    ("i8_value", "-8"),
    ("u16_value", "16"),
    ("i16_value", "-16"),
    ("u32_value", "32"),
    ("i32_value", "-32"),
    ("u64_value", "64"),
    ("i64_value", "-64"),
    ("f32_positive_value", "32.5"),
    ("f32_negative_value", "-32.5"),
    ("f64_positive_value", "64.25"),
    ("f64_negative_value", "-64.25"),
    ("string_value", r#""default string""#),
];

fn test_defaults(test: &mut Smoketest, publish_updated: impl FnOnce(&mut Smoketest)) {
    test.sql("INSERT INTO defaults_test_table (id) VALUES (1)").unwrap();
    publish_updated(test);

    for &(column, expected) in EXPECTED_DEFAULTS {
        let output = test
            .sql(&format!("SELECT {column} FROM defaults_test_table WHERE id = 1"))
            .unwrap();
        let actual = output.lines().last().unwrap().trim();
        assert_eq!(actual, expected, "incorrect default for column {column}");
    }
}

fn test_precompiled_defaults(project_name: &str) {
    let mut test = Smoketest::builder().autopublish(false).build();
    let database_name = format!("column-defaults-{project_name}-{}", random_string());
    let initial_project_name = format!("{project_name}-initial");
    let updated_project_name = format!("{project_name}-updated");
    test.use_precompiled_module(&initial_project_name);
    test.publish().name(&database_name).run().unwrap();

    test_defaults(&mut test, |test| {
        test.use_precompiled_module(&updated_project_name);
        test.publish()
            .current_database()
            .unwrap()
            .break_clients(true)
            .run()
            .unwrap();
    });
}

#[test]
fn test_rust_column_defaults() {
    let mut test = Smoketest::builder()
        .precompiled_module("column-defaults-initial")
        .build();
    test_defaults(&mut test, |test| {
        test.use_precompiled_module("column-defaults-updated");
        test.publish()
            .current_database()
            .unwrap()
            .break_clients(true)
            .run()
            .unwrap();
    });
}

#[test]
fn test_typescript_column_defaults() {
    test_precompiled_defaults("column-defaults-ts");
}

#[test]
fn test_csharp_column_defaults() {
    if !allow_dotnet() {
        return;
    }
    test_precompiled_defaults("column-defaults-csharp");
}

#[test]
fn test_cpp_column_defaults() {
    test_precompiled_defaults("column-defaults-cpp");
}
