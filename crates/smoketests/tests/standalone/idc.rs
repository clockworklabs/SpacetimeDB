use std::process::Command;
use std::time::{Duration, Instant};

use regex::Regex;
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{
    patch_module_cargo_to_local_bindings, random_string, require_local_server, workspace_root, Smoketest,
};

fn copy_module_fixture(name: &str, dest: &std::path::Path) {
    let source = workspace_root().join("crates/smoketests/modules").join(name);
    std::fs::create_dir_all(dest.join("src")).expect("failed to create copied module src dir");
    std::fs::copy(source.join("Cargo.toml"), dest.join("Cargo.toml")).expect("failed to copy Cargo.toml");
    std::fs::copy(source.join("src/lib.rs"), dest.join("src/lib.rs")).expect("failed to copy lib.rs");
}

fn parse_identity(publish_output: &str) -> String {
    Regex::new(r"identity: ([0-9a-fA-F]+)")
        .unwrap()
        .captures(publish_output)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .expect("publish output should contain database identity")
}

fn run_cli(test: &Smoketest, args: &[&str], cwd: &std::path::Path) -> String {
    let output = Command::new(ensure_binaries_built())
        .arg("--config-path")
        .arg(&test.config_path)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run spacetime");
    assert!(
        output.status.success(),
        "spacetime {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn idc_outbox_delivers_between_two_rust_modules() {
    require_local_server!();

    let test = Smoketest::builder().autopublish(false).build();
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let receiver_dir = temp_dir.path().join("receiver");
    let sender_dir = temp_dir.path().join("sender");
    let receiver_db = format!("idc-receiver-{}", random_string());
    let sender_db = format!("idc-sender-{}", random_string());

    copy_module_fixture("idc-receiver", &receiver_dir);
    copy_module_fixture("idc-sender", &sender_dir);
    patch_module_cargo_to_local_bindings(&receiver_dir).expect("failed to patch receiver Cargo.toml");
    patch_module_cargo_to_local_bindings(&sender_dir).expect("failed to patch sender Cargo.toml");

    std::fs::write(
        temp_dir.path().join("spacetime.json"),
        format!(
            r#"{{
  "server": "local",
  "children": [
    {{
      "database": "{receiver_db}",
      "module-path": "./receiver"
    }},
    {{
      "database": "{sender_db}",
      "module-path": "./sender",
      "dependencies": [
        {{ "name": "game_world", "database": "{receiver_db}" }}
      ]
    }}
  ]
}}
"#
        ),
    )
    .expect("failed to write spacetime.json");

    run_cli(&test, &["generate", &sender_db], temp_dir.path());

    let receiver_identity = parse_identity(&run_cli(
        &test,
        [
            "publish",
            "--no-config",
            "--server",
            &test.server_url,
            "--module-path",
            receiver_dir.to_str().unwrap(),
            "--yes=all",
            &receiver_db,
        ]
        .as_slice(),
        temp_dir.path(),
    ));
    let sender_identity = parse_identity(&run_cli(
        &test,
        [
            "publish",
            "--no-config",
            "--server",
            &test.server_url,
            "--module-path",
            sender_dir.to_str().unwrap(),
            "--yes=all",
            &sender_db,
        ]
        .as_slice(),
        temp_dir.path(),
    ));

    run_cli(
        &test,
        [
            "call",
            "--no-config",
            "--server",
            &test.server_url,
            "--",
            &sender_identity,
            "enqueue_ping",
            &format!("\"{receiver_identity}\""),
            "\"hello-idc\"",
        ]
        .as_slice(),
        temp_dir.path(),
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = run_cli(
            &test,
            [
                "sql",
                "--no-config",
                "--server",
                &test.server_url,
                &receiver_identity,
                "SELECT body FROM ping_log",
            ]
            .as_slice(),
            temp_dir.path(),
        );
        if rows.contains("hello-idc") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "IDC delivery did not appear in receiver rows:\n{rows}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = run_cli(
            &test,
            [
                "sql",
                "--no-config",
                "--server",
                &test.server_url,
                &sender_identity,
                "SELECT body FROM ping_result WHERE succeeded = true",
            ]
            .as_slice(),
            temp_dir.path(),
        );
        if rows.contains("hello-idc") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "IDC result callback did not record success in sender rows:\n{rows}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
