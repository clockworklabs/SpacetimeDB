use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn formats_downloaded_reports_and_reports_io_errors() {
    let root = std::env::temp_dir().join(format!(
        "llm-summary-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let reports = root.join("reports");
    fs::create_dir_all(&root).unwrap();
    let output = root.join("payload.json");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_ci-llm-benchmark-summary"))
            .arg("--reports-dir")
            .arg(&reports)
            .args(["--run-url", "https://example.com/run", "--run-label", "Manual run"])
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    let payload = || serde_json::from_slice::<Value>(&fs::read(&output).unwrap()).unwrap();

    // A failed benchmark job can leave no report artifact to download.
    assert!(run().status.success());
    assert_eq!(payload()["embeds"][0]["description"], "**0/0 (0.0%)** task runs passed");

    fs::create_dir_all(reports.join("nested")).unwrap();
    fs::write(
        reports.join("nested").join("typescript.md"),
        "# LLM Benchmark Analysis\r\n- Language: typescript\r\n- Mode: guidelines\r\n- Model: test-model\r\n- Tasks: 2/3 (66.7%)\r\n\r\n## Recommended actions\r\n\r\nNo repository changes recommended.\r\n\r\n## Failure patterns\r\n\r\n### API usage (1 task)\r\n\r\n- **Classification:** Model limitation\r\n",
    )
    .unwrap();
    fs::write(reports.join("rust.md"), "- Language: rust\n- Tasks: 1/1 (100.0%)\n").unwrap();
    fs::write(reports.join("ignored.json"), "not a report").unwrap();
    assert!(run().status.success());
    let result = payload();
    let embed = &result["embeds"][0];
    assert_eq!(embed["description"], "**3/4 (75.0%)** task runs passed");
    assert_eq!(
        embed["fields"][0]["value"],
        "- **Rust:** 1/1 (100.0%)\n- **TypeScript:** 2/3 (66.7%)"
    );
    assert_eq!(
        embed["fields"][2]["value"],
        "- TypeScript / guidelines / test-model: API usage - Model limitation"
    );

    let broken = reports.join("broken.md");
    fs::write(&broken, [0xff]).unwrap();
    let failure = run();
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("broken.md"));
    fs::remove_file(&broken).unwrap();
    fs::remove_file(&output).unwrap();
    fs::create_dir(&output).unwrap();
    let failure = run();
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("writing Discord payload"));
    fs::remove_dir_all(root).unwrap();
}
