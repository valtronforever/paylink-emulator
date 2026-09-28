use paylink_core::*;
#[test]
fn reference_errors_require_failure_and_preserve_bank_outcome() {
    assert!(
        Scenario {
            reference_error: Some("E21".into()),
            ..Scenario::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Scenario {
            reference_error: Some("E99".into()),
            outcome: Outcome::Declined,
            ..Scenario::default()
        }
        .validate()
        .is_err()
    );
    let mut e = Engine::default();
    e.arm(Scenario {
        reference_error: Some("E21".into()),
        outcome: Outcome::Declined,
        ..instant()
    })
    .unwrap();
    let id = e.start(DEVICE_ID, 100, None).unwrap();
    assert_eq!(e.result(&id).unwrap()["code"], 9307);
    assert_eq!(e.counters.approvals, 0);
}

#[test]
fn approved_shape_matches_recorded_paylink_with_simulated_ssi() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../profiles/desktop-paylink-2.1.20-win-x86/reference/approved.json"
    ))
    .unwrap();
    let date = chrono::NaiveDate::parse_from_str(
        fixture["evidence"]["capture_date"].as_str().unwrap(),
        "%Y-%m-%d",
    )
    .unwrap();
    let mut e = Engine::default().with_epoch(
        date.and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis() as u64,
    );
    e.arm(instant()).unwrap();
    let id = e.start(DEVICE_ID, 100, Some("TEST-MERCHANT")).unwrap();
    let mut actual = e.result(&id).unwrap();
    let mut expected = fixture["response"]["body"].clone();
    assert_eq!(
        actual["id"],
        actual["result"]["additional_properties"]["transactionUid"]
    );
    for path in fixture["variable_fields"].as_array().unwrap() {
        let pointer = format!("/{}", path.as_str().unwrap().replace('.', "/"));
        let a = actual.pointer_mut(&pointer).unwrap();
        let b = expected.pointer_mut(&pointer).unwrap();
        assert_eq!(a.is_string(), b.is_string(), "{pointer}");
        assert_eq!(a.is_number(), b.is_number(), "{pointer}");
        *a = serde_json::Value::Null;
        *b = serde_json::Value::Null;
    }
    for mapping in fixture["id_mapping"].as_array().unwrap() {
        for path in mapping["paths"].as_array().unwrap() {
            let pointer = format!("/{}", path.as_str().unwrap().replace('.', "/"));
            let value = actual.pointer_mut(&pointer).unwrap();
            assert_eq!(value, &mapping["emulator"]);
            *value = mapping["reference"].clone();
        }
    }
    assert_eq!(actual, expected);
}

fn instant() -> Scenario {
    Scenario {
        timing: Timing {
            connect_ms: 0,
            card_ms: 0,
            customer_ms: 0,
            authorize_ms: 0,
            confirm_ms: 0,
            response_ms: 0,
            timeout_ms: 120_000,
        },
        ..Scenario::default()
    }
}
#[test]
fn full_and_partial_amounts_are_exact_and_duplicates_are_not_hidden() {
    let mut e = Engine::default();
    e.arm(Scenario {
        uses: 2,
        ..instant()
    })
    .unwrap();
    let a = e.start(DEVICE_ID, 35000, None).unwrap();
    let b = e.start(DEVICE_ID, 25000, None).unwrap();
    assert_ne!(a, b);
    assert_eq!(e.counters.approvals, 2);
    assert_eq!(e.result(&b).unwrap()["result"]["amount"], 25000);
}
#[test]
fn timeout_wins_boundary_and_never_charges() {
    for delta in [119_999, 120_000, 120_001] {
        let mut e = Engine::default();
        e.arm(Scenario {
            timing: Timing {
                authorize_ms: 120_000,
                ..instant().timing
            },
            ..instant()
        })
        .unwrap();
        let id = e.start(DEVICE_ID, 100, None).unwrap();
        e.advance(delta).unwrap();
        assert_eq!(
            e.operations[&id].stage,
            if delta < 120_000 {
                Stage::Authorizing
            } else {
                Stage::TimedOut
            }
        );
        assert_eq!(e.counters.approvals, 0);
    }
}
#[test]
fn manual_events_busy_and_reconfiguration_are_guarded() {
    let mut e = Engine::default();
    e.arm(Scenario {
        mode: Mode::Manual,
        manual_bank: true,
        require_confirmation: true,
        ..instant()
    })
    .unwrap();
    let id = e.start(DEVICE_ID, 100, None).unwrap();
    assert_eq!(e.operations[&id].stage, Stage::AwaitingCard);
    assert!(e.action(&id, "bank_approved").is_err());
    assert_eq!(
        e.start(DEVICE_ID, 100, None).unwrap_err().code,
        "terminal_busy"
    );
    assert!(e.set_device(Device::default()).is_err());
    e.action(&id, "card_presented").unwrap();
    e.action(&id, "customer_confirmed").unwrap();
    e.action(&id, "bank_approved").unwrap();
    assert_eq!(e.counters.approvals, 0);
    e.action(&id, "terminal_confirmed").unwrap();
    assert_eq!(e.counters.approvals, 1);
    e.action(&id, "reversed").unwrap();
    assert_eq!(e.counters.reversals, 1);
    assert!(!e.operations[&id].charged);
}
#[test]
fn every_documented_terminal_error_has_a_scenario() {
    assert_eq!(catalog::ERRORS.len(), 13);
    for error in catalog::ERRORS {
        let mut e = Engine::default();
        let s = Scenario {
            outcome: Outcome::Error,
            error_id: Some(error.id.into()),
            ..instant()
        };
        if error.category != "terminal" {
            assert!(e.arm(s).is_err());
            continue;
        }
        e.arm(s).unwrap();
        let id = e.start(DEVICE_ID, 100, None).unwrap();
        assert_eq!(e.operations[&id].error_id.as_deref(), Some(error.id));
        assert_eq!(e.result(&id).unwrap()["success"], false);
        assert_eq!(e.counters.approvals, 0);
    }
}
#[test]
fn reset_and_replay_preserve_isolation() {
    let mut e = Engine::default();
    e.arm(Scenario {
        mode: Mode::Manual,
        ..instant()
    })
    .unwrap();
    let old = e.start(DEVICE_ID, 100, None).unwrap();
    e.reset();
    e.arm(instant()).unwrap();
    let new = e.start(DEVICE_ID, 100, None).unwrap();
    assert_ne!(old, new);
    assert!(e.action(&old, "card_presented").is_err());
    let saved = serde_json::to_string(&e).unwrap();
    let restored: Engine = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.result(&new).unwrap(), e.result(&new).unwrap());
}
#[test]
fn invalid_requests_do_not_consume_scenarios() {
    let mut e = Engine::default();
    e.arm(Scenario {
        amount: Some(100),
        ..instant()
    })
    .unwrap();
    for (id, amount, merchant) in [
        ("bad", 100, None),
        (DEVICE_ID, 0, None),
        (DEVICE_ID, 100, Some("wrong")),
        (DEVICE_ID, 101, None),
    ] {
        assert!(e.start(id, amount, merchant).is_err());
        assert_eq!(e.queue.len(), 1);
    }
    e.start(DEVICE_ID, 100, None).unwrap();
    assert!(e.queue.is_empty());
}
#[test]
fn setup_fault_is_unavailability_not_fake_payment_code() {
    let mut e = Engine::default();
    e.set_device(Device {
        setup_error: Some("driver_install_9011".into()),
        ..Device::default()
    })
    .unwrap();
    assert_eq!(
        e.start(DEVICE_ID, 100, None).unwrap_err().code,
        "terminal_connection_refused"
    );
    assert_eq!(e.counters.accepted, 0);
}
#[test]
fn defaults_are_strict_and_validation_rejects_impossible_scenarios() {
    let mut e = Engine::default();
    assert_eq!(
        e.start(DEVICE_ID, 100, None).unwrap_err().code,
        "scenario_missing"
    );
    assert!(
        e.arm(Scenario {
            outcome: Outcome::Error,
            ..Scenario::default()
        })
        .is_err()
    );
    assert!(
        e.arm(Scenario {
            failure_stage: Stage::AwaitingConfirmation,
            ..Scenario::default()
        })
        .is_err()
    );
    assert!(
        e.arm(Scenario {
            uses: 0,
            ..Scenario::default()
        })
        .is_err()
    );
}
#[test]
fn delivery_is_independent_of_bank_result() {
    let mut e = Engine::default();
    e.arm(Scenario {
        delivery: Delivery::DisconnectAfterCommit,
        ..instant()
    })
    .unwrap();
    let id = e.start(DEVICE_ID, 100, None).unwrap();
    assert_eq!(e.counters.approvals, 1);
    assert_eq!(e.counters.delivered, 0);
    e.delivered(&id).unwrap();
    e.delivered(&id).unwrap();
    assert_eq!(e.counters.delivered, 1);
}

#[test]
fn manual_input_cannot_bypass_selected_phase_error() {
    for (stage, events) in [
        (Stage::AwaitingCard, vec!["card_presented"]),
        (
            Stage::AwaitingCustomer,
            vec!["card_presented", "customer_confirmed"],
        ),
        (
            Stage::Authorizing,
            vec!["card_presented", "customer_confirmed", "bank_approved"],
        ),
    ] {
        let mut engine = Engine::default();
        engine
            .arm(Scenario {
                mode: Mode::Manual,
                manual_bank: true,
                outcome: Outcome::Error,
                error_id: Some("verification_fault".into()),
                failure_stage: stage,
                ..instant()
            })
            .unwrap();
        let id = engine.start(DEVICE_ID, 100, None).unwrap();
        for event in events {
            engine.action(&id, event).unwrap();
        }
        assert_eq!(engine.operations[&id].stage, Stage::Failed);
        assert_eq!(engine.counters.approvals, 0);
    }
}

#[test]
fn physical_disconnect_requires_device_reconnection_for_the_next_payment() {
    let mut engine = Engine::default();
    engine
        .arm(Scenario {
            mode: Mode::Manual,
            ..instant()
        })
        .unwrap();
    let id = engine.start(DEVICE_ID, 100, None).unwrap();
    engine.action(&id, "device_disconnected").unwrap();
    engine.arm(instant()).unwrap();
    assert_eq!(
        engine.start(DEVICE_ID, 100, None).unwrap_err().code,
        "terminal_connection_refused"
    );
    assert_eq!(engine.queue.len(), 1);
    engine.set_device(Device::default()).unwrap();
    engine.start(DEVICE_ID, 100, None).unwrap();
    assert_eq!(engine.counters.approvals, 1);
}

#[test]
fn receipt_references_do_not_collide_when_amount_and_sequence_offsets_cancel() {
    let mut engine = Engine::default();
    engine
        .arm(Scenario {
            uses: 2,
            ..instant()
        })
        .unwrap();
    let first = engine.start(DEVICE_ID, 100, None).unwrap();
    let second = engine.start(DEVICE_ID, 99, None).unwrap();
    assert_ne!(
        engine.result(&first).unwrap()["result"]["rrn"],
        engine.result(&second).unwrap()["result"]["rrn"]
    );
}
