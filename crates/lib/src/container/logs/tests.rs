use super::*;

fn page() -> ContainerLogPage {
    ContainerLogPage {
        database_identity: Identity::ONE,
        generation: u64::MAX,
        deployment_revision: Hash::from_byte_array([1; 32]),
        publication_operation: Uuid::from_u128(1),
        publication_epoch: u64::MAX,
        capture_id: Uuid::from_u128(2),
        records: vec![LogRecord {
            sequence: u64::MAX,
            event: LogEvent::Data {
                timestamp_micros: i64::MIN,
                stream: LogStream::Stderr,
                bytes: vec![0, 255, b'\n'],
            },
        }],
        next_cursor: "encoded_cursor".into(),
        oldest_retained_sequence: u64::MAX,
        retention_gap: true,
        has_more: false,
        end: Some(LogEnd::Eof),
        loss: Some(LogLoss::DrainTimeout),
    }
}

#[test]
fn pages_preserve_binary_output_and_exact_browser_integers() {
    let expected = page();
    expected.validate().unwrap();
    let value = serde_json::to_value(&expected).unwrap();
    assert_eq!(value["generation"], u64::MAX.to_string());
    assert_eq!(value["records"][0]["event"]["timestamp_micros"], i64::MIN.to_string());
    let decoded: ContainerLogPage = serde_json::from_value(value).unwrap();
    assert!(decoded == expected);
    assert_eq!(decoded.end, Some(LogEnd::Eof));
    assert_eq!(decoded.loss, Some(LogLoss::DrainTimeout));
}

#[test]
fn malformed_or_lossy_integer_and_capture_selections_are_rejected() {
    for generation in [
        serde_json::json!(u64::MAX),
        serde_json::json!("01"),
        serde_json::json!("-1"),
    ] {
        let mut value = serde_json::to_value(page()).unwrap();
        value["generation"] = generation;
        assert!(serde_json::from_value::<ContainerLogPage>(value).is_err());
    }
    for cursor in ["", "has space", "../other"] {
        let query = ContainerLogQuery {
            generation: Some(1),
            cursor: Some(cursor.into()),
            follow: true,
        };
        assert!(query.validate().is_err());
    }
    assert!(ContainerLogQuery {
        generation: None,
        cursor: Some("encoded".into()),
        follow: true
    }
    .validate()
    .is_err());
    let mut oversized = page();
    if let LogEvent::Data { bytes, .. } = &mut oversized.records[0].event {
        bytes.resize(MAX_LOG_RECORD_BYTES + 1, 0);
    }
    assert!(oversized.validate().is_err());
}
