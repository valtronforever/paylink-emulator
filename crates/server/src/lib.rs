//! Local control server and deliberately independent PayLink HTTP listener.
mod wire;
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use paylink_core::{Device, Engine, PROFILE, Scenario};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, watch},
    task::JoinHandle,
};

#[derive(Debug, Clone)]
pub struct Config {
    pub payment_addr: SocketAddr,
    pub control_addr: SocketAddr,
    pub token: String,
    pub controlled_clock: bool,
    pub allowed_origins: Vec<String>,
    pub journal: Option<PathBuf>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            payment_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000),
            control_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3001),
            token: String::new(),
            controlled_clock: false,
            allowed_origins: vec![],
            journal: None,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Shared {
    pub data: Arc<Mutex<Data>>,
    pub config: Arc<Config>,
    pub transport: watch::Sender<bool>,
    pub stop: watch::Sender<bool>,
    pub reset_connections: watch::Sender<u64>,
}
pub(crate) struct Data {
    pub engine: Engine,
    pub scenarios: BTreeMap<String, Scenario>,
    commands: BTreeMap<String, (Value, Value)>,
    pub listener_online: bool,
    pub listener_generation: u64,
    pub listener_error: Option<String>,
}
impl Shared {
    pub async fn journal(&self) -> Result<()> {
        let Some(path) = &self.config.journal else {
            return Ok(());
        };
        let data = self.data.lock().await;
        let report = json!({"captured_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),"engine":data.engine});
        // Serialize snapshots under the same lock so an older write cannot replace a newer one.
        let bytes = serde_json::to_vec_pretty(&report)?;
        let temp = path.with_extension("tmp");
        tokio::fs::write(&temp, bytes).await?;
        // Windows does not replace an existing target with rename.
        if cfg!(windows) && path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        tokio::fs::rename(temp, path).await?;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Ready {
    pub profile: &'static str,
    pub compatibility: &'static str,
    pub payment_url: String,
    pub control_url: String,
    pub clock: &'static str,
}
pub struct Server {
    pub ready: Ready,
    shared: Shared,
    tasks: Vec<JoinHandle<()>>,
}
impl Server {
    pub async fn start(config: Config) -> Result<Self> {
        if !config.control_addr.ip().is_loopback() || !config.payment_addr.ip().is_loopback() {
            bail!("both listeners must bind loopback; use a runner in the same network namespace");
        }
        if config.token.len() < 16 {
            bail!("control token must contain at least 16 characters");
        }
        for origin in &config.allowed_origins {
            if origin.contains(['\r', '\n'])
                || origin == "*"
                || !(origin.starts_with("http://") || origin.starts_with("https://"))
            {
                bail!("origins must be explicit HTTP(S) origins");
            }
        }
        let payment = TcpListener::bind(config.payment_addr)
            .await
            .context("bind payment listener")?;
        let control = TcpListener::bind(config.control_addr)
            .await
            .context("bind control listener")?;
        let payment_addr = payment.local_addr()?;
        let ready = Ready {
            profile: PROFILE,
            compatibility: "unverified",
            payment_url: format!("http://{payment_addr}"),
            control_url: format!("http://{}", control.local_addr()?),
            clock: if config.controlled_clock {
                "controlled"
            } else {
                "realtime"
            },
        };
        let (transport, _) = watch::channel(true);
        let (stop, _) = watch::channel(false);
        let (reset_connections, _) = watch::channel(1);
        let shared = Shared {
            data: Arc::new(Mutex::new(Data {
                engine: Engine::default()
                    .with_epoch(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64),
                scenarios: BTreeMap::new(),
                commands: BTreeMap::new(),
                listener_online: true,
                listener_generation: 1,
                listener_error: None,
            })),
            config: Arc::new(config),
            transport,
            stop,
            reset_connections,
        };
        let app = Router::new()
            .route("/control/v1/{resource}", get(read).post(command))
            .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
            .with_state(shared.clone());
        let mut stopped = shared.stop.subscribe();
        let control_task = tokio::spawn(async move {
            let _ = axum::serve(control, app)
                .with_graceful_shutdown(async move {
                    let _ = stopped.changed().await;
                })
                .await;
        });
        let s = shared.clone();
        let payment_task = tokio::spawn(async move {
            wire::serve(payment, payment_addr, s).await;
        });
        let s = shared.clone();
        let clock_task = tokio::spawn(async move {
            let mut stop = s.stop.subscribe();
            let mut interval = tokio::time::interval(Duration::from_millis(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut previous = Instant::now();
            loop {
                tokio::select! {
                    _ = stop.changed() => break,
                    _ = interval.tick() => {
                        let now = Instant::now();
                        let delta = now.duration_since(previous).as_millis() as u64;
                        if delta == 0 { continue; }
                        previous += Duration::from_millis(delta);
                        if !s.config.controlled_clock {
                            let changed = { let mut data=s.data.lock().await; let n=data.engine.events.len(); let _=data.engine.advance(delta); n!=data.engine.events.len() };
                            if changed && let Err(error)=s.journal().await { eprintln!("journal write failed: {error}"); }
                        }
                    }
                }
            }
        });
        Ok(Self {
            ready,
            shared,
            tasks: vec![control_task, payment_task, clock_task],
        })
    }
    pub async fn shutdown(mut self) -> Result<()> {
        self.shared.stop.send_replace(true);
        for mut task in self.tasks.drain(..) {
            if tokio::time::timeout(Duration::from_secs(2), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
        self.shared.journal().await
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.shared.stop.send_replace(true);
        for task in &self.tasks {
            task.abort();
        }
    }
}
fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"code":code,"message":message}})),
    )
        .into_response()
}
fn authorized(headers: &HeaderMap, s: &Shared) -> bool {
    // Native clients only. Browser access to control is intentionally forbidden, even with a token.
    !headers.contains_key("origin")
        && headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == format!("Bearer {}", s.config.token))
}
#[derive(Default, Deserialize)]
struct ReadQuery {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    wait_ms: u64,
}
async fn read(
    State(s): State<Shared>,
    Path(resource): Path<String>,
    Query(query): Query<ReadQuery>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers, &s) {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "native runner bearer token required",
        );
    }
    if resource == "events" && query.wait_ms > 0 {
        let deadline = Instant::now() + Duration::from_millis(query.wait_ms.min(30_000));
        loop {
            if s.data
                .lock()
                .await
                .engine
                .events
                .last()
                .is_some_and(|e| e.cursor > query.after)
                || Instant::now() >= deadline
                || *s.stop.borrow()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    let data = s.data.lock().await;
    let value = match resource.as_str() {
        "health" => json!({"ready":true,"profile":PROFILE,"generation":data.engine.generation}),
        "profile" => {
            json!({"id":PROFILE,"version":"2.1.20","platform":"win-x86","compatibility":"unverified","installer_sha256":"62d0e7a539380937ecb5af2f1c50438e4b84a070de4a20c1c848c11a5e395491"})
        }
        "errors" => json!(paylink_core::catalog::ERRORS),
        "state" | "journal" => json!(data.engine),
        "devices" => json!(data.engine.devices),
        "operations" => json!(data.engine.operations),
        "scenarios" => json!(data.scenarios),
        "queue" => json!(data.engine.queue),
        "events" => {
            json!({"events":data.engine.events.iter().filter(|e|e.cursor>query.after).collect::<Vec<_>>(),"generation":data.engine.generation,"cursor":data.engine.events.last().map(|e|e.cursor).unwrap_or(query.after)})
        }
        "transport" => json!({"online":data.listener_online,"error":data.listener_error}),
        _ => {
            return error(
                StatusCode::NOT_FOUND,
                "not_found",
                "unknown control resource",
            );
        }
    };
    Json(value).into_response()
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    command_id: String,
    #[serde(default)]
    generation: Option<u64>,
    #[serde(default)]
    payload: Value,
}
fn required_string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{key} must be a string"))
}
async fn command(
    State(s): State<Shared>,
    Path(resource): Path<String>,
    headers: HeaderMap,
    Json(cmd): Json<Command>,
) -> Response {
    if !authorized(&headers, &s) {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "native runner bearer token required",
        );
    }
    if cmd.command_id.is_empty() || cmd.command_id.len() > 128 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_command_id",
            "command_id must contain 1..128 characters",
        );
    }
    let signature = json!({"resource":resource,"payload":cmd.payload,"generation":cmd.generation});
    let mut data = s.data.lock().await;
    if let Some((old, result)) = data.commands.get(&cmd.command_id) {
        return if old == &signature {
            Json(result.clone()).into_response()
        } else {
            error(
                StatusCode::CONFLICT,
                "command_conflict",
                "command_id already used with different content",
            )
        };
    }
    if cmd.generation.is_some_and(|g| g != data.engine.generation) {
        return error(
            StatusCode::CONFLICT,
            "stale_generation",
            "reset invalidated this command",
        );
    }
    let mut transport_change = None;
    let mut reset_generation = None;
    let result = (|| -> Result<Value> {
        let p = &cmd.payload;
        match resource.as_str() {
            "scenarios" => {
                let scenario: Scenario = serde_json::from_value(p.clone())?;
                scenario.validate()?;
                let id = scenario.id.clone();
                data.scenarios.insert(id.clone(), scenario);
                Ok(json!({"saved":id}))
            }
            "arm" => {
                let scenario: Scenario =
                    if let Some(id) = p.get("scenario_id").and_then(Value::as_str) {
                        data.scenarios
                            .get(id)
                            .context("unknown saved scenario")?
                            .clone()
                    } else {
                        serde_json::from_value(p.clone())?
                    };
                data.engine.arm(scenario)?;
                Ok(json!({"queued":data.engine.queue.len()}))
            }
            "devices" => {
                let device: Device = serde_json::from_value(p.clone())?;
                data.engine.set_device(device)?;
                Ok(json!({"configured":true}))
            }
            "purchase" => {
                let id = required_string(p, "device_id")?;
                let amount = p
                    .get("amount")
                    .and_then(Value::as_u64)
                    .context("amount must be integer minor units")?;
                let op =
                    data.engine
                        .start(id, amount, p.get("merchant").and_then(Value::as_str))?;
                Ok(json!({"operation_id":op,"operation":data.engine.operations[&op]}))
            }
            "standalone" => {
                let scenario: Scenario = serde_json::from_value(
                    p.get("scenario").context("scenario required")?.clone(),
                )?;
                let amount = p
                    .get("amount")
                    .and_then(Value::as_u64)
                    .context("amount must be integer minor units")?;
                if data
                    .engine
                    .queue
                    .iter()
                    .any(|queued| queued.device_id == scenario.device_id)
                {
                    bail!(
                        "device has queued scenarios; consume or reset them before standalone start"
                    );
                }
                let device_id = scenario.device_id.clone();
                let mut next = data.engine.clone();
                next.arm(scenario)?;
                let operation_id = next.start(&device_id, amount, None)?;
                data.engine = next;
                Ok(
                    json!({"operation_id":operation_id,"operation":data.engine.operations[&operation_id]}),
                )
            }
            "action" => {
                let id = required_string(p, "operation_id")?;
                data.engine.action(id, required_string(p, "event")?)?;
                Ok(json!(data.engine.operations[id]))
            }
            "advance" => {
                if !s.config.controlled_clock {
                    bail!("advance requires controlled clock");
                }
                let delta = p
                    .get("ms")
                    .and_then(Value::as_u64)
                    .context("ms must be an integer")?;
                if delta > 3_600_000 {
                    bail!("advance at most one hour per command");
                }
                data.engine.advance(delta)?;
                Ok(json!({"now_ms":data.engine.now_ms}))
            }
            "transport" => {
                let online = p
                    .get("online")
                    .and_then(Value::as_bool)
                    .context("online must be boolean")?;
                transport_change = Some(online);
                Ok(json!({"requested_online":online}))
            }
            "reset" => {
                data.engine.reset();
                reset_generation = Some(data.engine.generation);
                data.scenarios.clear();
                data.commands.clear();
                transport_change = Some(true);
                Ok(json!({"generation":data.engine.generation}))
            }
            "assert" => {
                if p.get("queue_empty") == Some(&Value::Bool(true)) && !data.engine.queue.is_empty()
                {
                    bail!("unused scenarios remain");
                }
                if p.get("idle") == Some(&Value::Bool(true))
                    && data.engine.operations.values().any(|o| !o.stage.terminal())
                {
                    bail!("active operations remain");
                }
                for (key, actual) in [
                    ("requests", data.engine.counters.requests),
                    ("accepted", data.engine.counters.accepted),
                    ("approvals", data.engine.counters.approvals),
                    ("reversals", data.engine.counters.reversals),
                    ("delivered", data.engine.counters.delivered),
                ] {
                    if let Some(expected) = p.get(key)
                        && expected.as_u64() != Some(actual)
                    {
                        bail!("{key}: expected {expected}, actual {actual}");
                    }
                }
                Ok(json!({"pass":true,"counters":data.engine.counters}))
            }
            _ => bail!("unknown command"),
        }
    })();
    let result = match result {
        Ok(v) => v,
        Err(e) => return error(StatusCode::CONFLICT, "command_rejected", &e.to_string()),
    };
    data.commands
        .insert(cmd.command_id, (signature, result.clone()));
    drop(data);
    if let Some(generation) = reset_generation {
        s.reset_connections.send_replace(generation);
        let deadline = Instant::now() + Duration::from_secs(2);
        while s.data.lock().await.listener_generation != generation {
            if Instant::now() >= deadline {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "reset_failed",
                    "payment listener did not acknowledge reset",
                );
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    if let Some(online) = transport_change {
        s.transport.send_replace(online);
    }
    if let Err(e) = s.journal().await {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "journal_failed",
            &e.to_string(),
        );
    }
    Json(result).into_response()
}
