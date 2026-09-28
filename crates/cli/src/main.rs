use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use paylink_core::{DEVICE_ID, Delivery, Mode, Outcome, Scenario};
use paylink_server::{Config, Server};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
#[command(
    version,
    about = "Desktop PayLink 2.1.20 emulator — experimental wire compatibility"
)]
struct Cli {
    #[arg(
        long,
        env = "PAYLINK_CONTROL_URL",
        default_value = "http://127.0.0.1:3001",
        global = true
    )]
    control: String,
    #[arg(long, env = "PAYLINK_CONTROL_TOKEN", default_value = "", global = true)]
    token: String,
    #[command(subcommand)]
    command: CliCommand,
}
#[derive(Clone, Copy, ValueEnum)]
enum Clock {
    Realtime,
    Controlled,
}
#[derive(Clone, Copy, ValueEnum)]
enum State {
    Approved,
    Declined,
    Error,
    Manual,
}
#[derive(Subcommand)]
enum CliCommand {
    /// Run both listeners. Prints one JSON readiness record; Ctrl-C flushes journal.
    Serve {
        #[arg(long, default_value = "127.0.0.1:3000")]
        payment_addr: SocketAddr,
        #[arg(long, default_value = "127.0.0.1:3001")]
        control_addr: SocketAddr,
        #[arg(long, value_enum, default_value = "realtime")]
        clock: Clock,
        #[arg(long)]
        allow_origin: Vec<String>,
        #[arg(long)]
        journal: Option<PathBuf>,
        /// Queue a scenario from JSON before reporting readiness.
        #[arg(long)]
        scenario: Option<PathBuf>,
    },
    /// Read state, devices, errors, events, operations, profile, transport, or journal.
    Get {
        #[arg(default_value = "state")]
        resource: String,
    },
    /// Send any control command; payload is JSON or @path/to/file.json.
    Command {
        resource: String,
        #[arg(default_value = "{}")]
        payload: String,
        #[arg(long)]
        command_id: Option<String>,
        #[arg(long)]
        generation: Option<u64>,
    },
    /// Queue the next operation from convenient flags or a full JSON file.
    Arm {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "approved")]
        state: State,
        #[arg(long)]
        error_id: Option<String>,
        #[arg(long,default_value=DEVICE_ID)]
        device: String,
        #[arg(long)]
        amount: Option<u64>,
        #[arg(long, default_value_t = 1)]
        uses: u32,
        #[arg(long, default_value_t = 1200)]
        authorize_ms: u64,
        #[arg(long, default_value_t = 120000)]
        timeout_ms: u64,
        #[arg(long, default_value = "normal")]
        delivery: String,
    },
    /// Apply a physical terminal event (card_presented, customer_confirmed, ...).
    Action {
        operation_id: String,
        event: String,
    },
    /// Start a standalone terminal operation using the control API.
    Purchase {
        amount: u64,
        #[arg(long,default_value=DEVICE_ID)]
        device: String,
    },
    Advance {
        ms: u64,
    },
    Reset,
    /// Fail with nonzero exit status when journal expectations are not met.
    Assert {
        #[arg(default_value = "{\"queue_empty\":true,\"idle\":true}")]
        expected: String,
    },
    /// Export the current journal as JSON.
    Export {
        path: PathBuf,
    },
}
fn command_id() -> String {
    format!(
        "cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
fn parse_payload(s: &str) -> Result<Value> {
    Ok(if let Some(path) = s.strip_prefix('@') {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        serde_json::from_str(s)?
    })
}
async fn request(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    resource: &str,
    command: Option<Value>,
) -> Result<Value> {
    if resource.contains(['/', '?', '#']) {
        bail!("resource must be a simple control resource name");
    }
    let url = format!("{}/control/v1/{resource}", base.trim_end_matches('/'));
    let response = if let Some(body) = command {
        client.post(url).json(&body)
    } else {
        client.get(url)
    }
    .bearer_auth(token)
    .send()
    .await?;
    let status = response.status();
    let value: Value = response.json().await?;
    if !status.is_success() {
        bail!("{status}: {value}");
    }
    Ok(value)
}
async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result = tokio::signal::ctrl_c() => {result?;}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    anyhow::ensure!(
        args.token.len() >= 16,
        "set PAYLINK_CONTROL_TOKEN or --token (at least 16 characters)"
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(35))
        .build()?;
    let envelope = |payload: Value| json!({"command_id":command_id(),"payload":payload});
    let (resource, body) = match args.command {
        CliCommand::Serve {
            payment_addr,
            control_addr,
            clock,
            allow_origin,
            journal,
            scenario,
        } => {
            let server = Server::start(Config {
                payment_addr,
                control_addr,
                token: args.token.clone(),
                controlled_clock: matches!(clock, Clock::Controlled),
                allowed_origins: allow_origin,
                journal,
            })
            .await?;
            if let Some(path) = scenario {
                let payload: Value = serde_json::from_slice(&std::fs::read(path)?)?;
                request(
                    &client,
                    &server.ready.control_url,
                    &args.token,
                    "arm",
                    Some(envelope(payload)),
                )
                .await?;
            }
            println!("{}", serde_json::to_string(&server.ready)?);
            shutdown_signal().await?;
            server.shutdown().await?;
            return Ok(());
        }
        CliCommand::Get { resource } => (resource, None),
        CliCommand::Command {
            resource,
            payload,
            command_id: id,
            generation,
        } => (
            resource,
            Some(
                json!({"command_id":id.unwrap_or_else(command_id),"generation":generation,"payload":parse_payload(&payload)?}),
            ),
        ),
        CliCommand::Arm {
            file,
            state,
            error_id,
            device,
            amount,
            uses,
            authorize_ms,
            timeout_ms,
            delivery,
        } => {
            let scenario: Scenario = if let Some(path) = file {
                serde_json::from_slice(&std::fs::read(path)?)?
            } else {
                let mode = if matches!(state, State::Manual) {
                    Mode::Manual
                } else {
                    Mode::Automatic
                };
                let outcome = match state {
                    State::Error => Outcome::Error,
                    State::Declined => Outcome::Declined,
                    _ => Outcome::Approved,
                };
                let delivery: Delivery =
                    serde_json::from_value(json!(delivery)).context("unknown delivery policy")?;
                let mut scenario = Scenario {
                    mode,
                    outcome,
                    error_id,
                    device_id: device,
                    amount,
                    uses,
                    delivery,
                    ..Scenario::default()
                };
                scenario.timing.authorize_ms = authorize_ms;
                scenario.timing.timeout_ms = timeout_ms;
                scenario
            };
            scenario.validate()?;
            ("arm".into(), Some(envelope(json!(scenario))))
        }
        CliCommand::Action {
            operation_id,
            event,
        } => (
            "action".into(),
            Some(envelope(json!({"operation_id":operation_id,"event":event}))),
        ),
        CliCommand::Purchase { amount, device } => (
            "purchase".into(),
            Some(envelope(json!({"device_id":device,"amount":amount}))),
        ),
        CliCommand::Advance { ms } => ("advance".into(), Some(envelope(json!({"ms":ms})))),
        CliCommand::Reset => ("reset".into(), Some(envelope(json!({})))),
        CliCommand::Assert { expected } => {
            ("assert".into(), Some(envelope(parse_payload(&expected)?)))
        }
        CliCommand::Export { path } => {
            let value = request(&client, &args.control, &args.token, "journal", None).await?;
            std::fs::write(path, serde_json::to_vec_pretty(&value)?)?;
            return Ok(());
        }
    };
    let result = request(&client, &args.control, &args.token, &resource, body).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    #[test]
    fn command_tree_and_documented_flags_are_valid() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from([
            "paylink-emulator",
            "arm",
            "--state",
            "manual",
            "--authorize-ms",
            "500",
            "--token",
            "test-token-sixteen",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            CliCommand::Arm {
                state: State::Manual,
                authorize_ms: 500,
                ..
            }
        ));
    }
}
