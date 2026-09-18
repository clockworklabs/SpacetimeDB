use super::*;

fn start() -> ExecStart {
    ExecStart {
        generation: u64::MAX,
        argv: vec!["/bin/echo".into(), "secret-argv; $(literal)".into()],
        working_directory: Some("/secret-directory".into()),
        environment: BTreeMap::from([("TOKEN".into(), "secret-value".into())]),
        stdin: true,
        terminal: None,
    }
}

#[test]
fn start_preserves_literal_arguments_exact_generation_and_redacts_debug() {
    let message = ClientControl::Start(start());
    let json = serde_json::to_vec(&message).unwrap();
    assert!(String::from_utf8_lossy(&json).contains("\"generation\":\"18446744073709551615\""));
    assert_eq!(ClientControl::decode(&json).unwrap(), message);
    let debug = format!("{message:?}");
    for secret in ["secret-argv", "literal", "secret-directory", "TOKEN", "secret-value"] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn invalid_start_never_reflects_values_and_rejects_ambiguous_generation() {
    let baseline = serde_json::to_value(ClientControl::Start(start())).unwrap();
    for value in [
        serde_json::json!(1),
        serde_json::json!("01"),
        serde_json::json!("0"),
        serde_json::json!("-1"),
    ] {
        let mut changed = baseline.clone();
        changed["data"]["generation"] = value;
        assert!(ClientControl::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    let changes: [fn(&mut ExecStart); 12] = [
        |s| s.argv.clear(),
        |s| s.argv[0].clear(),
        |s| s.argv.push("secret-value\0".into()),
        |s| s.argv = vec!["x".into(); MAX_ARGV_ENTRIES + 1],
        |s| s.working_directory = Some("relative-secret-path".into()),
        |s| s.working_directory = Some("/secret\0".into()),
        |s| s.working_directory = Some(format!("/{}", "x".repeat(4096))),
        |s| {
            s.environment.insert("SPACETIMEDB_TOKEN".into(), "secret-value".into());
        },
        |s| {
            s.environment.insert("INVALID=KEY".into(), "secret-value".into());
        },
        |s| {
            s.environment.insert("TOKEN".into(), "secret\0".into());
        },
        |s| s.environment = (0..=MAX_ENV_KEYS).map(|i| (format!("E{i}"), String::new())).collect(),
        |s| s.terminal = Some(TerminalSize { rows: 0, columns: 80 }),
    ];
    for change in changes {
        let mut changed = start();
        change(&mut changed);
        let bytes = serde_json::to_vec(&ClientControl::Start(changed)).unwrap();
        let error = ClientControl::decode(&bytes).unwrap_err();
        assert_eq!(error.to_string(), "invalid container exec message");
        assert!(!format!("{error:?}").contains("secret"));
    }
}

#[test]
fn aggregate_budget_counts_overrides_and_json_bound_accepts_escaped_values() {
    let mut request = start();
    request.environment.clear();
    request.argv = vec!["program".into()];
    request.argv.extend(vec!["\u{1}".repeat(15_000); 8]);
    let bytes = serde_json::to_vec(&ClientControl::Start(request.clone())).unwrap();
    assert!(bytes.len() > 700_000 && bytes.len() < MAX_CONTROL_BYTES);
    assert!(ClientControl::decode(&bytes).is_ok());
    request.environment.insert("TOKEN".into(), "x".repeat(15_000));
    assert!(request.validate().is_err());
    request = start();
    request
        .environment
        .insert("TOKEN".into(), "x".repeat(super::super::MAX_EXEC_STRING_BYTES));
    assert!(request.validate().is_err());
    assert!(ClientControl::decode(&vec![b' '; MAX_CONTROL_BYTES + 1]).is_err());
}

#[test]
fn binary_frames_preserve_arbitrary_bytes_enforce_direction_and_bound_size() {
    let bytes = [0, 255, 128, b'\n'];
    let input = encode_data(Stream::Stdin, &bytes).unwrap();
    assert_eq!(decode_stdin(&input).unwrap(), bytes);
    assert!(decode_output(&input).is_err());
    for stream in [Stream::Stdout, Stream::Stderr] {
        let output = encode_data(stream, &bytes).unwrap();
        assert_eq!(decode_output(&output).unwrap(), (stream, bytes.as_slice()));
        assert!(decode_stdin(&output).is_err());
    }
    let max = encode_data(Stream::Stdin, &vec![0xff; MAX_DATA_BYTES]).unwrap();
    assert_eq!(max.len(), MAX_BINARY_BYTES);
    assert_eq!(decode_stdin(&max).unwrap().len(), MAX_DATA_BYTES);
    for invalid in [vec![], vec![0], vec![3, 1], vec![0; MAX_BINARY_BYTES + 1]] {
        assert!(decode_stdin(&invalid).is_err());
        assert!(decode_output(&invalid).is_err());
    }
    assert!(encode_data(Stream::Stdin, &[]).is_err());
    assert!(encode_data(Stream::Stdout, &vec![0; MAX_DATA_BYTES + 1]).is_err());
}

#[test]
fn eof_resize_and_signal_controls_are_distinct_and_checked() {
    let eof = br#"{"type":"stdin_eof"}"#;
    assert_eq!(ClientControl::decode(eof).unwrap(), ClientControl::StdinEof);
    assert!(decode_stdin(&[0]).is_err());
    for control in [
        ClientControl::Resize(TerminalSize { rows: 24, columns: 80 }),
        ClientControl::Signal(1),
        ClientControl::Signal(64),
    ] {
        assert_eq!(
            ClientControl::decode(&serde_json::to_vec(&control).unwrap()).unwrap(),
            control
        );
    }
    for control in [
        ClientControl::Resize(TerminalSize {
            rows: 4097,
            columns: 80,
        }),
        ClientControl::Signal(0),
        ClientControl::Signal(65),
    ] {
        assert!(ClientControl::decode(&serde_json::to_vec(&control).unwrap()).is_err());
    }
}

#[test]
fn protocol_rejects_unknown_duplicate_and_trailing_fields_without_echo() {
    for bad in [
        br#"{"type":"signal","data":2,"secret":"value"}"#.as_slice(),
        br#"{"type":"resize","data":{"rows":24,"columns":80,"secret":"value"}}"#,
        br#"{"type":"signal","type":"stdin_eof","data":2}"#,
        br#"{"type":"signal","data":2} {"secret":"value"}"#,
        br#"{"type":"not-a-command","data":"secret-value"}"#,
    ] {
        assert_eq!(ClientControl::decode(bad).unwrap_err(), ProtocolError);
    }
}

#[test]
fn server_messages_keep_exact_session_metadata_and_reject_invalid_ready() {
    let ready = ExecReady {
        database_identity: Identity::from_byte_array([7; 32]),
        generation: u64::MAX,
        session_id: Uuid::from_u128(42),
        tty: true,
    };
    for message in [
        ServerControl::Ready(ready.clone()),
        ServerControl::Exit { exit_code: 137 },
        ServerControl::Error {
            error: ContainerErrorCode::AccessDenied,
        },
    ] {
        assert_eq!(
            ServerControl::decode(&serde_json::to_vec(&message).unwrap()).unwrap(),
            message
        );
    }
    for changed in [
        ExecReady {
            generation: 0,
            ..ready.clone()
        },
        ExecReady {
            session_id: Uuid::from_u128(0),
            ..ready
        },
    ] {
        assert!(ServerControl::decode(&serde_json::to_vec(&ServerControl::Ready(changed)).unwrap()).is_err());
    }
    assert!(ServerControl::decode(br#"{"type":"exit","data":{"exit_code":0,"secret":"value"}}"#).is_err());
    assert!(ServerControl::decode(&vec![b' '; 4097]).is_err());
}
