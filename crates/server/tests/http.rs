use paylink_core::{DEVICE_ID, Delivery, Mode, Scenario, Timing, catalog::ERRORS};
use paylink_server::{Config, Server};
use serde_json::{Value, json};
use std::time::Duration;
const TOKEN: &str = "test-run-isolated-token";
struct Harness {
    server: Server,
    client: reqwest::Client,
    seq: u64,
}
impl Harness {
    async fn new(controlled: bool) -> Self {
        let server = Server::start(Config {
            payment_addr: "127.0.0.1:0".parse().unwrap(),
            control_addr: "127.0.0.1:0".parse().unwrap(),
            token: TOKEN.into(),
            controlled_clock: controlled,
            allowed_origins: vec!["https://example.test".into()],
            journal: None,
        })
        .await
        .unwrap();
        Self {
            server,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            seq: 0,
        }
    }
    fn endpoint(&self, path: &str) -> String {
        format!("{}/control/v1/{path}", self.server.ready.control_url)
    }
    async fn command(&mut self, path: &str, payload: Value) -> Value {
        self.seq += 1;
        let r = self
            .client
            .post(self.endpoint(path))
            .bearer_auth(TOKEN)
            .json(&json!({"command_id":format!("c{}",self.seq),"payload":payload}))
            .send()
            .await
            .unwrap();
        let status = r.status();
        let body: Value = r.json().await.unwrap();
        assert!(status.is_success(), "{path}: {status} {body}");
        body
    }
    async fn read(&self, path: &str) -> Value {
        self.client
            .get(self.endpoint(path))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    async fn arm(&mut self, s: Scenario) {
        self.command("arm", json!(s)).await;
    }
    fn payment(&self) -> reqwest::RequestBuilder {
        self.client
            .post(format!(
                "{}/api/pos/{DEVICE_ID}/purchase",
                self.server.ready.payment_url
            ))
            .json(&json!({"amount":100}))
    }
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
#[tokio::test]
async fn authentication_origin_and_idempotent_control() {
    let h = Harness::new(true).await;
    assert_eq!(
        h.client
            .get(h.endpoint("state"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        h.client
            .get(h.endpoint("state"))
            .bearer_auth(TOKEN)
            .header("Origin", "https://example.test")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let payload = json!({"command_id":"same-command","payload":instant()});
    for _ in 0..2 {
        assert_eq!(
            h.client
                .post(h.endpoint("arm"))
                .bearer_auth(TOKEN)
                .json(&payload)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    let changed = json!({"command_id":"same-command","payload":{"id":"other"}});
    assert_eq!(
        h.client
            .post(h.endpoint("arm"))
            .bearer_auth(TOKEN)
            .json(&changed)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(h.read("queue").await.as_array().unwrap().len(), 1);
    let url = format!("{}/api/devices/", h.server.ready.payment_url);
    let preflight = h
        .client
        .request(reqwest::Method::OPTIONS, &url)
        .header("Origin", "https://example.test")
        .header("Access-Control-Request-Method", "POST")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), 204);
    assert_eq!(
        preflight.headers()["access-control-allow-origin"],
        "https://example.test"
    );
    assert_eq!(
        h.client
            .get(url)
            .header("Origin", "https://evil.test")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    h.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn every_terminal_error_travels_over_real_http_and_recovers() {
    let mut h = Harness::new(true).await;
    for error in ERRORS.iter().filter(|e| e.category == "terminal") {
        h.arm(Scenario {
            outcome: paylink_core::Outcome::Error,
            error_id: Some(error.id.into()),
            ..instant()
        })
        .await;
        let body: Value = h.payment().send().await.unwrap().json().await.unwrap();
        assert_eq!(body["success"], false, "{}", error.id);
        assert_eq!(body["error"], error.message);
        h.arm(instant()).await;
        let recovered: Value = h.payment().send().await.unwrap().json().await.unwrap();
        assert_eq!(recovered["success"], true);
    }
    h.command(
        "assert",
        json!({"approvals":11,"accepted":22,"queue_empty":true,"idle":true}),
    )
    .await;
    h.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn real_transport_faults_preserve_bank_outcome() {
    let mut h = Harness::new(true).await;
    for delivery in [
        Delivery::DisconnectBeforeAccept,
        Delivery::DisconnectAfterCommit,
        Delivery::PartialResponse,
        Delivery::MalformedJson,
        Delivery::Http500,
    ] {
        h.command("reset", json!({})).await;
        h.arm(Scenario {
            delivery,
            ..instant()
        })
        .await;
        let response = h.payment().send().await;
        match delivery {
            Delivery::DisconnectBeforeAccept | Delivery::DisconnectAfterCommit => {
                assert!(response.is_err())
            }
            Delivery::PartialResponse | Delivery::MalformedJson => {
                assert!(response.unwrap().json::<Value>().await.is_err())
            }
            Delivery::Http500 => assert_eq!(response.unwrap().status(), 500),
            _ => unreachable!(),
        }
        let state = h.read("state").await;
        assert_eq!(
            state["counters"]["approvals"],
            if delivery == Delivery::DisconnectBeforeAccept {
                0
            } else {
                1
            }
        );
        assert_eq!(state["counters"]["delivered"], 0);
    }
    h.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn controlled_time_manual_actions_busy_reset_and_listener_recovery() {
    let mut h = Harness::new(true).await;
    h.arm(Scenario {
        mode: Mode::Manual,
        ..instant()
    })
    .await;
    let request = h.payment();
    let payment = tokio::spawn(async move { request.send().await });
    let id = loop {
        let ops = h.read("operations").await;
        if let Some((id, _)) = ops.as_object().unwrap().iter().next() {
            break id.clone();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let busy: Value = h.payment().send().await.unwrap().json().await.unwrap();
    assert_eq!(busy["error"], "Device is busy");
    h.command(
        "action",
        json!({"operation_id":id,"event":"card_presented"}),
    )
    .await;
    h.command(
        "action",
        json!({"operation_id":id,"event":"customer_confirmed"}),
    )
    .await;
    assert_eq!(
        payment
            .await
            .unwrap()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["success"],
        true
    );
    h.command("transport", json!({"online":false})).await;
    for _ in 0..100 {
        if h.read("transport").await["online"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(h.payment().send().await.is_err());
    assert_eq!(h.read("state").await["counters"]["approvals"], 1);
    h.command("reset", json!({})).await;
    for _ in 0..100 {
        if h.read("transport").await["online"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    h.arm(Scenario {
        delivery: Delivery::Hang,
        ..instant()
    })
    .await;
    let request = h.payment();
    let hanging = tokio::spawn(async move { request.send().await });
    for _ in 0..100 {
        if h.read("state").await["counters"]["approvals"] == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    h.command("reset", json!({})).await;
    assert!(hanging.await.unwrap().is_err());
    h.command("assert", json!({"approvals":0,"requests":0}))
        .await;
    h.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn realtime_delay_is_observable_and_client_timeout_does_not_cancel_charge() {
    let mut h = Harness::new(false).await;
    h.arm(Scenario {
        timing: Timing {
            authorize_ms: 180,
            response_ms: 100,
            ..instant().timing
        },
        ..instant()
    })
    .await;
    let before = std::time::Instant::now();
    assert!(
        h.payment()
            .timeout(Duration::from_millis(40))
            .send()
            .await
            .is_err()
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(before.elapsed() >= Duration::from_millis(280));
    h.command("assert", json!({"approvals":1,"accepted":1,"idle":true}))
        .await;
    h.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn independent_process_state_and_controlled_delay_boundary() {
    let mut a = Harness::new(true).await;
    let b = Harness::new(true).await;
    a.arm(Scenario {
        timing: Timing {
            authorize_ms: 100,
            ..instant().timing
        },
        ..instant()
    })
    .await;
    let request = a.payment();
    let payment = tokio::spawn(async move { request.send().await });
    for _ in 0..100 {
        if a.read("state").await["counters"]["accepted"] == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    a.command("advance", json!({"ms":99})).await;
    assert!(!payment.is_finished());
    assert_eq!(b.read("state").await["counters"]["requests"], 0);
    a.command("advance", json!({"ms":1})).await;
    assert_eq!(
        payment
            .await
            .unwrap()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["success"],
        true
    );
    a.server.shutdown().await.unwrap();
    b.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn setup_fault_is_visible_without_claiming_a_payment_driver_code() {
    let mut h = Harness::new(true).await;
    h.command("devices",json!({"id":DEVICE_ID,"name":"Virtual POS","merchant":"TEST-MERCHANT","online":true,"setup_error":"driver_install_9011"})).await;
    let response: Value = h.payment().send().await.unwrap().json().await.unwrap();
    assert_eq!(response["success"], false);
    assert_eq!(response["error"], "Connection refused");
    h.command("assert", json!({"accepted":0,"approvals":0}))
        .await;
    h.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn synthetic_response_shapes_and_disconnect_after_accept_are_distinct() {
    let mut h = Harness::new(true).await;
    for delivery in [
        Delivery::Http400,
        Delivery::WrongContentType,
        Delivery::MissingFields,
        Delivery::UnknownCode,
    ] {
        h.command("reset", json!({})).await;
        h.arm(Scenario {
            delivery,
            ..instant()
        })
        .await;
        let response = h.payment().send().await.unwrap();
        if delivery == Delivery::Http400 {
            assert_eq!(response.status(), 400);
        }
        if delivery == Delivery::WrongContentType {
            assert_eq!(response.headers()["content-type"], "text/plain");
        }
        let body: Value = response.json().await.unwrap();
        if delivery == Delivery::MissingFields {
            assert_eq!(body["result"], json!({}));
        }
        if delivery == Delivery::UnknownCode {
            assert_eq!(body["code"], 999999);
        }
        h.command("assert", json!({"approvals":1,"accepted":1,"delivered":0}))
            .await;
    }
    h.command("reset", json!({})).await;
    h.arm(Scenario {
        delivery: Delivery::DisconnectAfterAccept,
        timing: Timing {
            authorize_ms: 100,
            ..instant().timing
        },
        ..instant()
    })
    .await;
    assert!(h.payment().send().await.is_err());
    h.command("assert", json!({"accepted":1,"approvals":0}))
        .await;
    h.command("advance", json!({"ms":100})).await;
    h.command("assert", json!({"accepted":1,"approvals":1,"delivered":0}))
        .await;
    h.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn reset_closes_incomplete_requests_before_a_new_scenario_can_be_consumed() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut h = Harness::new(true).await;
    let addr = h.server.ready.payment_url.strip_prefix("http://").unwrap();
    let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    socket.write_all(format!("POST /api/pos/{DEVICE_ID}/purchase HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 14\r\n\r\n{{").as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    h.command("reset", json!({})).await;
    h.arm(instant()).await;
    let mut bytes = [0; 1024];
    let read = tokio::time::timeout(Duration::from_secs(1), socket.read(&mut bytes))
        .await
        .unwrap();
    assert!(matches!(read, Ok(0) | Err(_)));
    h.command("assert", json!({"requests":0,"approvals":0}))
        .await;
    assert_eq!(h.read("queue").await.as_array().unwrap().len(), 1);
    h.server.shutdown().await.unwrap();
}
