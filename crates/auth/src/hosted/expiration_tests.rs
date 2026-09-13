use super::*;

fn proof() -> (VerifiedHostedAuth, SystemTime, Instant) {
    let wall = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let monotonic = Instant::now();
    (
        VerifiedHostedAuth {
            claims: HostedTokenClaims {
                kind: HOSTED_TOKEN_KIND.into(),
                issuer: "platform.test".into(),
                subject: Identity::ZERO.to_hex().to_string().into(),
                source_database: Identity::ZERO,
                target_database: Identity::ZERO,
                generation: 1,
                grant_revision: 2,
                issued_at: 1_700_000_000,
                expires_at: 1_700_000_030,
                token_id: "expiration-test".into(),
            },
            monotonic_deadline: monotonic + Duration::from_secs(30),
        },
        wall,
        monotonic,
    )
}

#[test]
fn receiving_clock_behind_control_cannot_extend_confirmed_lifetime() {
    let (proof, wall, monotonic) = proof();
    let request_started = monotonic - Duration::from_secs(2);
    let proof = proof
        .constrain_expiration(wall + Duration::from_secs(25), request_started)
        .unwrap();
    let deadline = request_started + Duration::from_secs(5);
    assert_eq!(proof.monotonic_deadline, deadline);
    assert!(proof
        .check_at_clocks(wall + Duration::from_secs(3), deadline - Duration::from_nanos(1))
        .is_ok());
    assert!(proof.check_at_clocks(wall + Duration::from_secs(3), deadline).is_err());
    assert_eq!(
        proof.remaining_lifetime_at_clocks(wall + Duration::from_secs(3), deadline),
        Duration::ZERO
    );
}

#[test]
fn confirmation_round_trip_consumes_remaining_signed_lifetime() {
    let (proof, wall, monotonic) = proof();
    assert!(proof
        .constrain_expiration(wall + Duration::from_secs(25), monotonic - Duration::from_secs(6))
        .is_err());
}

#[test]
fn later_confirmation_cannot_extend_a_previous_deadline() {
    let (proof, wall, monotonic) = proof();
    let proof = proof
        .constrain_expiration(wall + Duration::from_secs(25), monotonic)
        .unwrap();
    let first_deadline = proof.monotonic_deadline;
    // Even a second response with an earlier authority timestamp cannot extend
    // the stricter deadline already held by this authentication proof.
    let proof = proof
        .constrain_expiration(wall + Duration::from_secs(20), Instant::now())
        .unwrap();
    assert_eq!(proof.monotonic_deadline, first_deadline);
}

#[test]
fn backward_wall_clock_does_not_restart_the_local_lifetime() {
    let (proof, wall, monotonic) = proof();
    let later = monotonic + Duration::from_secs(29);
    assert_eq!(
        proof.remaining_lifetime_at_clocks(wall + Duration::from_secs(1), later),
        Duration::from_secs(1)
    );
    assert!(proof
        .check_at_clocks(wall + Duration::from_secs(1), monotonic + Duration::from_secs(30))
        .is_err());
}

#[test]
fn wall_clock_expiration_remains_an_independent_limit() {
    let (proof, wall, monotonic) = proof();
    assert_eq!(
        proof.remaining_lifetime_at_clocks(wall + Duration::from_secs(29), monotonic),
        Duration::from_secs(1)
    );
    assert!(proof
        .check_at_clocks(wall + Duration::from_secs(30), monotonic)
        .is_err());
    assert_eq!(
        proof.remaining_lifetime_at_clocks(wall - Duration::from_secs(1), monotonic),
        Duration::ZERO
    );
}

#[test]
fn invalid_confirmation_clocks_are_rejected() {
    let (proof, wall, monotonic) = proof();
    assert!(proof
        .clone()
        .constrain_expiration(wall - Duration::from_secs(1), monotonic)
        .is_err());
    assert!(proof
        .clone()
        .constrain_expiration(wall + Duration::from_secs(30), monotonic)
        .is_err());
    assert!(proof
        .constrain_expiration(wall, Instant::now() + Duration::from_secs(60))
        .is_err());
}

#[test]
fn cloning_and_connection_conversion_preserve_the_cap_and_signed_claims() {
    let (proof, wall, monotonic) = proof();
    let original_claims = serde_json::to_value(&proof.claims).unwrap();
    let proof = proof
        .constrain_expiration(wall + Duration::from_secs(25), monotonic)
        .unwrap();
    let deadline = proof.monotonic_deadline;
    let cloned = proof.clone();
    let connection = proof.into_connection_auth().unwrap();
    let connection = connection.clone();
    let retained = connection.hosted.as_ref().unwrap();
    assert_eq!(cloned.monotonic_deadline, deadline);
    assert_eq!(retained.monotonic_deadline, deadline);
    assert_eq!(connection.claims.exp, Some(wall + Duration::from_secs(30)));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&connection.jwt_payload).unwrap(),
        original_claims
    );
    assert!(retained
        .check_at_clocks(wall + Duration::from_secs(1), deadline)
        .is_err());
}
