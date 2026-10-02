use spacetimedb_smoketests::Smoketest;

/// Check describing a module, as human-readable text and as JSON
#[test]
fn test_describe() {
    let test = Smoketest::builder().precompiled_module("describe").build();

    let identity = test.database_identity.as_ref().unwrap().as_str();
    let describe = |args: &[&str]| -> String {
        let full = [&["describe", "--server", test.server_url.as_str()][..], args].concat();
        test.spacetime(&full).unwrap()
    };

    // Describe the whole module: text by default, uncoloured because stdout is piped, and the
    // unstable warning goes to stderr.
    let text = describe(&[identity]);
    for expected in [
        "Tables",
        "person (private)",
        "name",
        "String",
        "Reducers",
        "add(name: String)",
        "say_hello()",
        "Views",
        "HTTP routes",
        "Environment variables",
    ] {
        assert!(text.contains(expected), "expected {expected:?} in:\n{text}");
    }
    assert!(
        !text.contains('\x1b'),
        "piped describe output has ANSI escapes: {text:?}"
    );
    assert!(
        !text.contains("UNSTABLE"),
        "unstable warning leaked into stdout:\n{text}"
    );

    // Describing a single entity prints only that entity.
    assert_eq!(
        describe(&[identity, "tables", "person"]),
        "person (private)\n  Columns:\n    name  String\n"
    );
    assert_eq!(describe(&[identity, "reducers", "say_hello"]), "say_hello()\n");
    assert_eq!(
        describe(&[identity, "views", "nobody"]),
        "nobody() -> Option<Person>  [public]\n"
    );
    assert_eq!(describe(&[identity, "routes", "/health"]), "GET /health → health\n");
    assert_eq!(
        describe(&[identity, "env", "LANGUAGE"]),
        "LANGUAGE  \"en\" | \"fr\"  optional\n"
    );

    // An entity that doesn't exist is an error, not empty output.
    for entity in [["views", "missing"], ["routes", "/missing"], ["env", "MISSING"]] {
        let args = [
            &["describe", "--server", test.server_url.as_str(), identity][..],
            &entity[..],
        ]
        .concat();
        let out = test.spacetime_cmd(&args);
        assert!(!out.status.success(), "describe {entity:?} should fail");
    }

    // `--format json` is the same as `--json`, for the whole module and for each entity.
    let entities: [&[&str]; 9] = [
        &[],
        &["tables", "person"],
        &["reducers", "say_hello"],
        &["views"],
        &["views", "nobody"],
        &["routes"],
        &["routes", "/health"],
        &["env"],
        &["env", "LANGUAGE"],
    ];
    for entity in entities {
        let parse = |flag: &[&str]| -> serde_json::Value {
            let args = [flag, &[identity], entity].concat();
            let out = describe(&args);
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("describe {args:?} is not JSON: {e}\n{out}"))
        };
        assert_eq!(parse(&["--json"]), parse(&["--format", "json"]), "entity {entity:?}");
    }

    // `--json` conflicts with an explicit `--format`.
    let out = test.spacetime_cmd(&[
        "describe",
        "--server",
        &test.server_url,
        "--json",
        "--format",
        "text",
        identity,
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "`--json --format text` should be a usage error"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cannot be used with"), "unexpected stderr: {stderr}");
}
