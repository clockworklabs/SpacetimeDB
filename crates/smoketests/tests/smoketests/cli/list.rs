//! CLI list command tests

use spacetimedb_smoketests::{require_local_server, Smoketest};
use std::process::Output;

fn output_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn output_stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed:\nstdout: {}\nstderr: {}",
        output_stdout(output),
        output_stderr(output),
    );
}

#[test]
fn cli_list_shows_database_names_and_identities() {
    require_local_server!();
    let mut test = Smoketest::builder().autopublish(false).build();

    let primary_name = format!("list-db-{}", std::process::id());
    let alias_name = format!("{primary_name}-alias");
    let second_alias_name = format!("{primary_name}-alt");
    let identity = test.publish_module_named(&primary_name, false).unwrap();

    let json_body = format!(r#"["{}","{}"]"#, alias_name, second_alias_name);
    let response = test
        .api_call_json("PUT", &format!("/v1/database/{primary_name}/names"), &json_body)
        .unwrap();
    assert_eq!(
        response.status_code,
        200,
        "Expected 200 status when replacing names, got {}: {}",
        response.status_code,
        String::from_utf8_lossy(&response.body)
    );

    let output = test.spacetime_cmd(&["list", "--server", &test.server_url]);
    assert_success(&output, "spacetime list");

    let stdout = output_stdout(&output);
    assert!(
        stdout.contains("Database Name(s)"),
        "missing Database Name(s) column:\n{stdout}"
    );
    assert!(stdout.contains("Identity"), "missing Identity column:\n{stdout}");
    assert!(stdout.contains(&alias_name), "missing alias name in output:\n{stdout}");
    assert!(
        stdout.contains(&second_alias_name),
        "missing second alias name in output:\n{stdout}"
    );
    assert!(
        stdout.contains(&identity),
        "missing database identity in output:\n{stdout}"
    );
}
