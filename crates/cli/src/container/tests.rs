use super::*;
use crate::spacetime_config::SpacetimeConfig;
use serde_json::json;
use std::{collections::BTreeMap, sync::Mutex};

static TEST_PREPARATIONS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn platform() -> ImagePlatform {
    ImagePlatform {
        os: "linux".into(),
        architecture: "amd64".into(),
    }
}
pub(crate) fn declaration(image: serde_json::Value) -> ContainerConfig {
    serde_json::from_value(json!({"image":image,"resources":{"cpu_millicores":100,"memory_bytes":67108864,"scratch_bytes":1048576,"pids_max":32}})).unwrap()
}
fn blob(layout: &Path, bytes: &[u8], media: &str) -> Descriptor {
    let digest = spacetimedb_oci::sha256(bytes);
    fs::create_dir_all(layout.join("blobs/sha256")).unwrap();
    fs::write(
        layout
            .join("blobs/sha256")
            .join(digest.to_string().strip_prefix("sha256:").unwrap()),
        bytes,
    )
    .unwrap();
    Descriptor {
        media_type: media.into(),
        digest,
        size: bytes.len() as u64,
        platform: None,
        urls: vec![],
        data: None,
        artifact_type: None,
    }
}
pub(crate) fn fixture(layout: &Path) -> Descriptor {
    let mut archive = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append_data(&mut header, "app.js", &b"code"[..]).unwrap();
    let layer = archive.into_inner().unwrap();
    let layer = blob(layout, &layer, "application/vnd.oci.image.layer.v1.tar");
    let config = blob(layout, &serde_json::to_vec(&json!({"architecture":"amd64","os":"linux","config":{"Entrypoint":["/usr/bin/env"],"Cmd":["node","app.js"],"User":"1000:1000","WorkingDir":"/app","Env":["BAKED=private-image-value"]},"rootfs":{"type":"layers","diff_ids":[layer.digest]}})).unwrap(), spacetimedb_oci::OCI_CONFIG);
    let manifest = blob(
        layout,
        &serde_json::to_vec(
            &json!({"schemaVersion":2,"mediaType":spacetimedb_oci::OCI_MANIFEST,"config":config,"layers":[layer]}),
        )
        .unwrap(),
        spacetimedb_oci::OCI_MANIFEST,
    );
    fs::write(layout.join("oci-layout"), br#"{"imageLayoutVersion":"1.0.0"}"#).unwrap();
    fs::write(
        layout.join("index.json"),
        serde_json::to_vec(&json!({"schemaVersion":2,"mediaType":spacetimedb_oci::OCI_INDEX,"manifests":[manifest]}))
            .unwrap(),
    )
    .unwrap();
    manifest
}
fn copy_layout(source: &Path, output: &Path) {
    fs::create_dir_all(output.join("blobs/sha256")).unwrap();
    for name in ["oci-layout", "index.json"] {
        fs::copy(source.join(name), output.join(name)).unwrap();
    }
    for file in fs::read_dir(source.join("blobs/sha256")).unwrap() {
        let file = file.unwrap();
        fs::copy(file.path(), output.join("blobs/sha256").join(file.file_name())).unwrap();
    }
}
type RecordedCall = (String, Vec<OsString>, Vec<OsString>);

struct FakeRunner {
    layout: PathBuf,
    calls: Mutex<Vec<RecordedCall>>,
    fail_detection: bool,
    version: &'static str,
}
impl FakeRunner {
    fn new(layout: &Path) -> Self {
        Self {
            layout: layout.into(),
            calls: Mutex::new(vec![]),
            fail_detection: false,
            version: "railpack 0.35.0\n",
        }
    }
}
impl Runner for FakeRunner {
    async fn run(&self, invocation: Invocation) -> Result<process::Output> {
        self.calls.lock().unwrap().push((
            invocation.label.into(),
            invocation.args.clone(),
            invocation.env.iter().map(|(name, _)| name.clone()).collect(),
        ));
        match invocation.label {
            "Railpack version check" => {
                return Ok(process::Output {
                    stdout: self.version.as_bytes().to_vec(),
                })
            }
            "Railpack detection" => {
                ensure!(!self.fail_detection, "unsupported source detection");
                fs::write(invocation.workspace.path().join("railpack-plan.json"), b"{}")?;
            }
            "Skopeo image import" => copy_layout(&self.layout, &invocation.workspace.path().join("input")),
            "BuildKit OCI build" => {
                let file = fs::File::create(invocation.workspace.path().join("image.tar"))?;
                let mut tar = tar::Builder::new(file);
                for name in ["oci-layout", "index.json"] {
                    tar.append_path_with_name(self.layout.join(name), name)?;
                }
                for blob in fs::read_dir(self.layout.join("blobs/sha256"))? {
                    let blob = blob?;
                    tar.append_path_with_name(blob.path(), Path::new("blobs/sha256").join(blob.file_name()))?;
                }
                tar.finish()?;
            }
            _ => anyhow::bail!("unexpected fake image tool"),
        }
        Ok(process::Output { stdout: vec![] })
    }
}

#[test]
fn image_source_is_exclusive_and_builder_defaults_are_strict() {
    for image in [
        json!({}),
        json!({"build":{},"oci_ref":"example/image"}),
        json!({"build":{"builder":"unknown"}}),
        json!({"build":{"builder":"railpack","dockerfile":"Dockerfile"}}),
    ] {
        let mut config = serde_json::to_value(declaration(json!({"oci_ref":"example/image"}))).unwrap();
        config["image"] = image;
        assert!(serde_json::from_value::<ContainerConfig>(config).is_err());
    }
    assert!(matches!(
        declaration(json!({"build":{}})).image,
        ImageSource::Build(config::BuildImage {
            build: SourceBuild::Dockerfile { .. }
        })
    ));
}

#[test]
fn container_declarations_never_inherit_through_children_or_overrides() {
    let a = serde_json::to_value(declaration(json!({"build":{}}))).unwrap();
    let b = serde_json::to_value(declaration(json!({"oci_ref":"example/child"}))).unwrap();
    let config: SpacetimeConfig = serde_json::from_value(json!({"database":"root","module-path":"shared","container":a,"children":[{"database":"plain","children":[{"database":"grandchild"}]},{"database":"own","container":b,"children":[{"database":"own-grandchild"}]}]})).unwrap();
    let targets = config.collect_all_targets_with_inheritance();
    let declarations: BTreeMap<_, _> = targets
        .iter()
        .map(|target| (target.fields["database"].as_str().unwrap(), target.container.is_some()))
        .collect();
    assert_eq!(
        declarations,
        BTreeMap::from([
            ("root", true),
            ("plain", false),
            ("grandchild", false),
            ("own", true),
            ("own-grandchild", false)
        ])
    );
    assert!(targets.iter().all(|target| !target.fields.contains_key("container")));
    assert!(crate::subcommands::container::select(&config, Some("plain")).is_err());
    assert!(crate::subcommands::container::select(&config, Some("not-a-local-target")).is_err());
    assert!(crate::subcommands::container::select(&config, None).is_err());
    assert!(crate::subcommands::container::select(&config, Some("own")).is_ok());
}

#[tokio::test]
async fn local_prebuilt_output_is_owned_verified_and_keeps_image_defaults() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("prebuilt image");
    let manifest = fixture(&input);
    let runner = FakeRunner::new(&input);
    let token = CancellationToken::new();
    let prepared = prepare_container(
        &declaration(json!({"oci_ref":"oci:prebuilt image"})),
        root.path(),
        platform(),
        &BuildTools::default(),
        root.path(),
        &runner,
        token.clone(),
    )
    .await
    .unwrap();
    assert!(!token.is_cancelled());
    assert!(runner.calls.lock().unwrap().is_empty());
    assert_eq!(prepared.metadata.manifest.digest, manifest.digest);
    assert_eq!(prepared.metadata.container.argv, ["/usr/bin/env", "node", "app.js"]);
    assert_eq!(prepared.metadata.container.user, "1000:1000");
    assert_eq!(prepared.metadata.container.working_directory, "/app");
    assert!(!serde_json::to_string(&prepared.metadata)
        .unwrap()
        .contains("private-image-value"));
    assert_eq!(prepared.metadata.objects.len(), 3);
    let temporary = prepared.layout();
    let output = root.path().join("reusable");
    prepared.persist(&output).unwrap();
    assert!(!temporary.exists());
    assert!(output.join("prepared.json").is_file());
    let reread = prepare_container(
        &declaration(json!({"oci_ref":"oci:reusable"})),
        root.path(),
        platform(),
        &BuildTools::default(),
        root.path(),
        &runner,
        token,
    )
    .await
    .unwrap();
    assert_eq!(reread.metadata.manifest.digest, manifest.digest);
    let temporary = reread.layout();
    drop(reread);
    assert!(!temporary.exists());
}

#[tokio::test]
async fn command_override_replaces_image_argv_and_invalid_bytes_never_persist() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("image");
    let manifest = fixture(&input);
    let runner = FakeRunner::new(&input);
    let mut config = declaration(json!({"oci_ref":"oci:image"}));
    config.command = Some(vec!["/bin/custom".into()]);
    let prepared = prepare_container(
        &config,
        root.path(),
        platform(),
        &BuildTools::default(),
        root.path(),
        &runner,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(prepared.metadata.container.argv, ["/bin/custom"]);
    drop(prepared);
    fs::write(
        input
            .join("blobs/sha256")
            .join(manifest.digest.to_string().strip_prefix("sha256:").unwrap()),
        b"tampered",
    )
    .unwrap();
    assert!(prepare_container(
        &config,
        root.path(),
        platform(),
        &BuildTools::default(),
        root.path(),
        &runner,
        CancellationToken::new()
    )
    .await
    .is_err());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".spacetime-image-")));
}

#[tokio::test]
async fn dockerfile_and_railpack_have_explicit_local_endpoint_and_separate_secrets() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("fixture");
    fixture(&input);
    let context = root.path().join("source with spaces");
    fs::create_dir(&context).unwrap();
    fs::write(context.join("Dockerfile"), b"FROM scratch").unwrap();
    let secret = root.path().join("secret file");
    fs::write(&secret, b"sensitive-build-value").unwrap();
    for builder in ["dockerfile", "railpack"] {
        let runner = FakeRunner::new(&input);
        let tools = BuildTools {
            buildkit_host: Some("unix:///disposable/fake-buildkit.sock".into()),
            secrets: vec![BuildSecret {
                name: "BUILD_KEY".into(),
                file: secret.clone(),
            }],
            ..Default::default()
        };
        let config = declaration(json!({"build":{"builder":builder,"context":"source with spaces"}}));
        let prepared = prepare_container(
            &config,
            root.path(),
            platform(),
            &tools,
            root.path(),
            &runner,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        let calls = runner.calls.lock().unwrap();
        let (_, argv, environment) = calls
            .iter()
            .find(|(label, _, _)| label == "BuildKit OCI build")
            .unwrap();
        assert_eq!(&argv[..2], ["--addr", "unix:///disposable/fake-buildkit.sock"]);
        assert!(argv.contains(&format!("context={}", context.canonicalize().unwrap().display()).into()));
        assert!(argv.contains(&"--no-cache".into()));
        assert!(environment.contains(&"DOCKER_CONFIG".into()));
        assert!(!format!("{argv:?}").contains("sensitive-build-value"));
        assert!(!serde_json::to_string(&prepared.metadata).unwrap().contains("BUILD_KEY"));
        if builder == "railpack" {
            assert!(argv.contains(&format!("source={RAILPACK_FRONTEND}").into()));
            let (_, args, _) = calls
                .iter()
                .find(|(label, _, _)| label == "Railpack detection")
                .unwrap();
            assert!(args.contains(&"BUILD_KEY=".into()));
        } else {
            assert_eq!(calls.len(), 1);
        }
    }
}

#[tokio::test]
async fn unsupported_railpack_and_remote_builder_never_fall_back() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("image");
    fixture(&input);
    let config = declaration(json!({"build":{"builder":"railpack"}}));
    let tools = BuildTools {
        buildkit_host: Some("unix:///disposable/fake-buildkit.sock".into()),
        ..Default::default()
    };
    let mut runner = FakeRunner::new(&input);
    runner.fail_detection = true;
    assert!(prepare_container(
        &config,
        root.path(),
        platform(),
        &tools,
        root.path(),
        &runner,
        CancellationToken::new()
    )
    .await
    .is_err());
    assert_eq!(runner.calls.lock().unwrap().len(), 2);
    runner.calls.lock().unwrap().clear();
    runner.version = "railpack 99.0.0";
    assert!(prepare_container(
        &config,
        root.path(),
        platform(),
        &tools,
        root.path(),
        &runner,
        CancellationToken::new()
    )
    .await
    .is_err());
    assert_eq!(runner.calls.lock().unwrap().len(), 1);
    runner.calls.lock().unwrap().clear();
    let tools = BuildTools {
        buildkit_host: Some("tcp://untrusted:1234".into()),
        ..Default::default()
    };
    assert!(prepare_container(
        &config,
        root.path(),
        platform(),
        &tools,
        root.path(),
        &runner,
        CancellationToken::new()
    )
    .await
    .is_err());
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn registry_import_uses_explicit_anonymous_auth_and_records_selected_digest() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("image");
    let manifest = fixture(&input);
    let runner = FakeRunner::new(&input);
    let prepared = prepare_container(
        &declaration(json!({"oci_ref":"example.invalid/team/image:tag"})),
        root.path(),
        platform(),
        &BuildTools::default(),
        root.path(),
        &runner,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(prepared.metadata.manifest.digest, manifest.digest);
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let args = &calls[0].1;
    assert!(args.contains(&"--preserve-digests".into()));
    assert!(args.contains(&"docker://example.invalid/team/image:tag".into()));
    let auth = PathBuf::from(&args[args.iter().position(|arg| arg == "--authfile").unwrap() + 1]);
    assert_eq!(fs::read(auth).unwrap(), br#"{"auths":{}}"#);
}

#[test]
fn archive_rejects_links_and_cancelled_reads() {
    let root = tempfile::tempdir().unwrap();
    for kind in [tar::EntryType::Symlink, tar::EntryType::Link] {
        let path = root.path().join("input.tar");
        let mut tar = tar::Builder::new(fs::File::create(&path).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(kind);
        header.set_size(0);
        header.set_link_name("/outside").unwrap();
        header.set_cksum();
        tar.append_data(&mut header, "index.json", std::io::empty()).unwrap();
        tar.finish().unwrap();
        let output = tempfile::tempdir().unwrap();
        assert!(oci::extract_archive(
            &path,
            output.path(),
            &CancellationToken::new(),
            Instant::now() + Duration::from_secs(1)
        )
        .is_err());
    }
    let token = CancellationToken::new();
    token.cancel();
    assert!(oci::check(&token, Instant::now() + Duration::from_secs(1)).is_err());
}

#[test]
fn container_build_command_requires_explicit_platform_and_output() {
    crate::subcommands::container::cli().debug_assert();
    assert!(crate::subcommands::container::cli()
        .try_get_matches_from(["container", "build"])
        .is_err());
    assert!(crate::subcommands::container::cli()
        .try_get_matches_from([
            "container",
            "build",
            "local-target",
            "--platform",
            "linux/amd64",
            "--out-dir",
            "output"
        ])
        .is_ok());
}

#[test]
fn publish_preserves_container_target_metadata_without_affecting_plain_children() {
    use crate::subcommands::publish::{build_publish_schema, get_filtered_publish_configs};
    let config: SpacetimeConfig = serde_json::from_value(json!({
        "database":"container-db", "container":declaration(json!({"build":{}})),
        "children":[{"database":"ordinary-db"}]
    }))
    .unwrap();
    let command = crate::subcommands::publish::cli();
    let schema = build_publish_schema(&command).unwrap();
    for selected in ["container-db", "*"] {
        let args = command.clone().try_get_matches_from(["publish", selected]).unwrap();
        let targets = get_filtered_publish_configs(&config, &command, &schema, &args).unwrap();
        assert!(targets[0].container().is_some());
        assert!(targets.iter().skip(1).all(|target| target.container().is_none()));
    }
    let args = command
        .clone()
        .try_get_matches_from(["publish", "ordinary-db"])
        .unwrap();
    assert_eq!(
        get_filtered_publish_configs(&config, &command, &schema, &args)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn local_index_defaults_media_type_but_never_overwrites_existing_output() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("image");
    fixture(&input);
    let mut index: serde_json::Value = serde_json::from_slice(&fs::read(input.join("index.json")).unwrap()).unwrap();
    index.as_object_mut().unwrap().remove("mediaType");
    fs::write(input.join("index.json"), serde_json::to_vec(&index).unwrap()).unwrap();
    let prepared = prepare_container(
        &declaration(json!({"oci_ref":"oci:image"})),
        root.path(),
        platform(),
        &BuildTools::default(),
        root.path(),
        &FakeRunner::new(&input),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let output = root.path().join("existing-empty-output");
    fs::create_dir(&output).unwrap();
    assert!(prepared.persist(&output).is_err());
    assert!(fs::read_dir(&output).unwrap().next().is_none());
}

#[test]
fn wrong_platform_and_duplicate_archive_object_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("image");
    fixture(&input);
    let wrong = ImagePlatform {
        os: "linux".into(),
        architecture: "arm64".into(),
    };
    assert!(oci::verify_layout(
        &input,
        &root.path().join("output"),
        &wrong,
        &CancellationToken::new(),
        Instant::now() + Duration::from_secs(2)
    )
    .is_err());
    let archive = root.path().join("duplicate.tar");
    let mut tar = tar::Builder::new(fs::File::create(&archive).unwrap());
    for _ in 0..2 {
        tar.append_path_with_name(input.join("index.json"), "index.json")
            .unwrap();
    }
    tar.finish().unwrap();
    assert!(oci::extract_archive(
        &archive,
        &root.path().join("duplicate"),
        &CancellationToken::new(),
        Instant::now() + Duration::from_secs(2)
    )
    .is_err());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[tokio::test]
async fn subprocess_exit_cancellation_caller_drop_and_output_overflow_reap_before_workspace_release() {
    use process::LocalRunner;
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    // These scripts exercise only owned local subprocesses. No Docker, server,
    // network, saved configuration, or user credentials are involved.
    for mode in ["exit", "cancel", "drop", "overflow", "timeout"] {
        let workspace = Arc::new(
            tempfile::Builder::new()
                .prefix("fake-tool-")
                .tempdir_in(root.path())
                .unwrap(),
        );
        let path = workspace.path().to_path_buf();
        let pid_file = root.path().join(format!("{mode}.pid"));
        let script = match mode {
            "exit" => "echo $$ > \"$1\"; sleep 60 & exit 0",
            "overflow" => "echo $$ > \"$1\"; yes x",
            _ => "echo $$ > \"$1\"; sleep 60 & wait",
        };
        let token = CancellationToken::new();
        let task = tokio::spawn(LocalRunner.run(Invocation {
            tool: "/bin/sh".into(),
            label: "owned fake builder",
            args: vec![
                "-c".into(),
                script.into(),
                "fake-builder".into(),
                pid_file.clone().into_os_string(),
            ],
            env: vec![],
            cwd: root.path().into(),
            workspace,
            timeout: if mode == "timeout" {
                Duration::from_millis(250)
            } else {
                Duration::from_secs(4)
            },
            cancel: token.clone(),
        }));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !pid_file.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid = fs::read_to_string(&pid_file).unwrap().trim().parse::<i32>().unwrap();
        let pid = rustix::process::Pid::from_raw(pid).unwrap();
        if mode == "cancel" {
            token.cancel();
        }
        if mode == "drop" {
            task.abort();
            let _ = task.await;
        } else {
            let result = tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(result.is_ok(), mode == "exit");
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            matches!(
                rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG),
                Err(rustix::io::Errno::CHILD)
            ),
            "builder leader must already be reaped"
        );
    }
}

#[test]
fn configuration_overlay_replaces_only_the_selected_container_declaration() {
    let root = tempfile::tempdir().unwrap();
    let parent = declaration(json!({"build":{}}));
    let child = declaration(json!({"oci_ref":"example.invalid/child:v1"}));
    fs::write(root.path().join("spacetime.json"),serde_json::to_vec(&json!({"database":"parent","container":parent,"children":[{"database":"plain"},{"database":"own","container":child}]})).unwrap()).unwrap();
    let replacement = declaration(json!({"build":{"builder":"railpack"}}));
    fs::write(
        root.path().join("spacetime.dev.json"),
        serde_json::to_vec(&json!({"container":replacement})).unwrap(),
    )
    .unwrap();
    let loaded = crate::spacetime_config::find_and_load_with_env_from(Some("dev"), root.path().into())
        .unwrap()
        .unwrap();
    assert!(matches!(
        crate::subcommands::container::select(&loaded.config, Some("parent"))
            .unwrap()
            .image,
        ImageSource::Build(config::BuildImage {
            build: SourceBuild::Railpack { .. }
        })
    ));
    assert!(crate::subcommands::container::select(&loaded.config, Some("plain")).is_err());
    assert!(matches!(
        crate::subcommands::container::select(&loaded.config, Some("own"))
            .unwrap()
            .image,
        ImageSource::Prebuilt(_)
    ));
}

#[tokio::test]
async fn build_command_executes_local_import_with_only_project_configuration() {
    let _serial = TEST_PREPARATIONS.lock().await;
    let root = tempfile::tempdir().unwrap();
    fixture(&root.path().join("input"));
    fs::write(
        root.path().join("spacetime.json"),
        serde_json::to_vec(&json!({
            "database":"local-selection", "server":"https://must-not-be-contacted.invalid",
            "container":declaration(json!({"oci_ref":"oci:input"}))
        }))
        .unwrap(),
    )
    .unwrap();
    let output = root.path().join("output");
    let args = crate::subcommands::container::cli()
        .try_get_matches_from([
            OsString::from("container"),
            "build".into(),
            "local-selection".into(),
            "--project-path".into(),
            root.path().into(),
            "--out-dir".into(),
            output.clone().into_os_string(),
            "--platform".into(),
            "linux/amd64".into(),
        ])
        .unwrap();
    crate::exec_local_subcommand("container", &args).await.unwrap().unwrap();
    let metadata: PreparedMetadata = serde_json::from_slice(&fs::read(output.join("prepared.json")).unwrap()).unwrap();
    assert_eq!(metadata.container.argv, ["/usr/bin/env", "node", "app.js"]);
    assert_eq!(metadata.objects.len(), 3);
}

#[test]
fn omitted_image_working_directory_normalizes_to_linux_root() {
    let declaration = declaration(json!({"oci_ref":"example.invalid/no-working-dir"}));
    let image = spacetimedb_oci::ContainerConfig {
        cmd: Some(vec!["/app".into()]),
        ..Default::default()
    };
    let normalized = declaration
        .normalize(spacetimedb_oci::sha256(b"fixture"), platform(), &image)
        .unwrap();
    assert_eq!(normalized.working_directory, "/");
    let mut explicit = declaration;
    explicit.working_directory = Some(String::new());
    assert!(explicit
        .normalize(spacetimedb_oci::sha256(b"fixture"), platform(), &image)
        .is_err());
}
