use paylink_server::{Config, Server};
use serde_json::{Value, json};
async fn cli(server: &Server, args: &[&str]) -> std::process::Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_paylink-emulator"))
        .env("PAYLINK_CONTROL_TOKEN", "cli-integration-token")
        .arg("--control")
        .arg(&server.ready.control_url)
        .args(args)
        .output()
        .await
        .unwrap()
}
#[tokio::test]
async fn cli_drives_same_api_and_failed_assertions_exit_nonzero() {
    let server = Server::start(Config {
        payment_addr: "127.0.0.1:0".parse().unwrap(),
        control_addr: "127.0.0.1:0".parse().unwrap(),
        token: "cli-integration-token".into(),
        controlled_clock: true,
        ..Config::default()
    })
    .await
    .unwrap();
    let scenario = json!({"mode":"manual","timing":{"connect_ms":0,"authorize_ms":0}}).to_string();
    assert!(
        cli(&server, &["command", "arm", &scenario])
            .await
            .status
            .success()
    );
    let started = cli(&server, &["purchase", "1299"]).await;
    assert!(started.status.success(), "{:?}", started);
    let body: Value = serde_json::from_slice(&started.stdout).unwrap();
    let id = body["operation_id"].as_str().unwrap();
    for event in ["card_presented", "customer_confirmed"] {
        assert!(cli(&server, &["action", id, event]).await.status.success());
    }
    assert!(
        cli(
            &server,
            &["assert", "{\"approvals\":1,\"queue_empty\":true}"]
        )
        .await
        .status
        .success()
    );
    let failed = cli(&server, &["assert", "{\"approvals\":2}"]).await;
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("actual 1"));
    let state = cli(&server, &["get", "state"]).await;
    let body: Value = serde_json::from_slice(&state.stdout).unwrap();
    assert_eq!(body["operations"][id]["amount"], 1299);
    assert!(cli(&server, &["reset"]).await.status.success());
    server.shutdown().await.unwrap();
}
