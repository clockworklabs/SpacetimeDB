use std::process::Command;

pub fn derive_cat_task_from_file(src: &str) -> (String, String) {
    let p = std::path::Path::new(src);
    let task = p
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let cat = p
        .parent()
        .and_then(|d| d.parent())
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    (cat, task)
}

pub fn sql_exec(db: &str, query: &str, host: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("spacetime");
    cmd.arg("sql").arg(db).arg(query);
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }
    let out = cmd.output().map_err(|e| format!("spawn spacetime sql failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "spacetime sql failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

pub fn normalize(s: &str, collapse_ws: bool) -> String {
    let t = s.trim();
    if collapse_ws {
        t.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        t.to_string()
    }
}
