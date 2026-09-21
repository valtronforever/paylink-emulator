//! Native terminal skin. All mutations travel through the same API as CLI/tests.
use clap::Parser;
use gpui_kit::{
    component::{button::*, *},
    *,
};
use paylink_core::{Delivery, Device, Engine, Mode, Outcome, Scenario, Stage, catalog::ERRORS};
use paylink_server::{Config, Server};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

#[derive(Parser)]
#[command(version, about = "Interactive PayLink terminal simulator")]
struct Args {
    /// Attach to an existing headless server instead of starting an embedded one.
    #[arg(long)]
    connect: Option<String>,
    #[arg(long, env = "PAYLINK_CONTROL_TOKEN")]
    token: Option<String>,
    #[arg(long, default_value = "127.0.0.1:3000")]
    payment_addr: std::net::SocketAddr,
    #[arg(long, default_value = "127.0.0.1:3001")]
    control_addr: std::net::SocketAddr,
    #[arg(long)]
    allow_origin: Vec<String>,
    #[arg(long)]
    journal: Option<std::path::PathBuf>,
}
#[derive(Clone, Default)]
struct Snapshot {
    engine: Option<Engine>,
    message: String,
    connected: bool,
}
struct Api {
    tx: mpsc::Sender<(String, Value)>,
    snapshot: Arc<Mutex<Snapshot>>,
}
impl Api {
    fn new(base: String, token: String) -> Self {
        let (tx, rx) = mpsc::channel::<(String, Value)>();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared = snapshot.clone();
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap();
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok((resource, payload)) => {
                        let result = (|| -> anyhow::Result<Value> {
                            let response=client.post(format!("{base}/control/v1/{resource}")).bearer_auth(&token).json(&json!({"command_id":uuid::Uuid::new_v4().to_string(),"payload":payload})).send()?;
                            let status = response.status();
                            let value: Value = response.json()?;
                            anyhow::ensure!(status.is_success(), "{value}");
                            Ok(value)
                        })();
                        shared.lock().unwrap().message = match result {
                            Ok(_) => format!("{resource}: OK"),
                            Err(e) => format!("{resource}: {e}"),
                        };
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let result = client
                    .get(format!("{base}/control/v1/state"))
                    .bearer_auth(&token)
                    .send()
                    .and_then(|r| r.error_for_status())
                    .and_then(|r| r.json::<Engine>());
                let mut snapshot = shared.lock().unwrap();
                match result {
                    Ok(engine) => {
                        snapshot.engine = Some(engine);
                        snapshot.connected = true;
                    }
                    Err(e) => {
                        snapshot.connected = false;
                        snapshot.message = format!("Control connection: {e}");
                    }
                }
            }
        });
        Self { tx, snapshot }
    }
    fn send(&self, resource: &str, payload: Value) {
        let _ = self.tx.send((resource.into(), payload));
    }
}
#[derive(Clone, Copy)]
enum InputTarget {
    Amount,
    Connect,
    Card,
    Customer,
    Authorize,
    Response,
    Timeout,
}
impl InputTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Amount => "Amount (minor units)",
            Self::Connect => "Connect delay (ms)",
            Self::Card => "Card delay (ms)",
            Self::Customer => "Customer delay (ms)",
            Self::Authorize => "Bank delay (ms)",
            Self::Response => "Response delay (ms)",
            Self::Timeout => "Timeout (ms)",
        }
    }
}
struct Terminal {
    api: Api,
    focus: FocusHandle,
    amount: u64,
    scenario: Scenario,
    outcome: usize,
    input: InputTarget,
    replace_input: bool,
    base: String,
    device_index: usize,
    entry_dirty: bool,
    last_operation_id: Option<String>,
}
impl Terminal {
    fn new(api: Api, base: String, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if view.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            api,
            focus: cx.focus_handle(),
            amount: 100,
            scenario: Scenario {
                mode: Mode::Manual,
                ..Scenario::default()
            },
            outcome: 0,
            input: InputTarget::Amount,
            replace_input: true,
            base,
            device_index: 0,
            entry_dirty: true,
            last_operation_id: None,
        }
    }
    fn value(&mut self) -> &mut u64 {
        match self.input {
            InputTarget::Amount => &mut self.amount,
            InputTarget::Connect => &mut self.scenario.timing.connect_ms,
            InputTarget::Card => &mut self.scenario.timing.card_ms,
            InputTarget::Customer => &mut self.scenario.timing.customer_ms,
            InputTarget::Authorize => &mut self.scenario.timing.authorize_ms,
            InputTarget::Response => &mut self.scenario.timing.response_ms,
            InputTarget::Timeout => &mut self.scenario.timing.timeout_ms,
        }
    }
    fn digit(&mut self, ch: &str) {
        if matches!(self.input, InputTarget::Amount)
            && self.current().is_some_and(|o| !o.stage.terminal())
        {
            return;
        }
        if matches!(self.input, InputTarget::Amount) {
            self.entry_dirty = true;
        }
        let replace = self.replace_input;
        self.replace_input = false;
        let value = self.value();
        if ch == "clear" {
            *value = 0;
        } else if ch == "backspace" {
            *value /= 10;
        } else if let Ok(d) = ch.parse::<u64>() {
            *value = if replace {
                d
            } else {
                value.saturating_mul(10).saturating_add(d).min(999_999_999)
            };
        }
    }
    fn current(&self) -> Option<paylink_core::Operation> {
        let snapshot = self.api.snapshot.lock().unwrap();
        snapshot
            .engine
            .as_ref()?
            .operations
            .values()
            .filter(|o| o.device_id == self.scenario.device_id)
            .max_by_key(|o| {
                (
                    o.started_ms,
                    o.id.split("-op")
                        .last()
                        .and_then(|n| n.parse::<u64>().ok())
                        .unwrap_or(0),
                )
            })
            .cloned()
    }
    fn event(&self, event: &str) {
        if let Some(op) = self.current() {
            self.api
                .send("action", json!({"operation_id":op.id,"event":event}));
        }
    }
    fn configured_device(&self) -> Device {
        self.api
            .snapshot
            .lock()
            .unwrap()
            .engine
            .as_ref()
            .and_then(|e| e.devices.get(&self.scenario.device_id))
            .cloned()
            .unwrap_or_else(|| Device {
                id: self.scenario.device_id.clone(),
                ..Device::default()
            })
    }
    fn queue(&mut self, standalone: bool) {
        if self.outcome >= 2 {
            let error = &ERRORS[self.outcome - 2];
            if error.category == "transport" {
                self.api.send("transport", json!({"online":false}));
                return;
            }
            if error.category == "setup_only" {
                self.api.send(
                    "devices",
                    json!(Device {
                        id: self.scenario.device_id.clone(),
                        setup_error: Some(error.id.into()),
                        ..self.configured_device()
                    }),
                );
                return;
            }
            self.scenario.outcome = Outcome::Error;
            self.scenario.error_id = Some(error.id.into());
        } else {
            self.scenario.outcome = if self.outcome == 0 {
                Outcome::Approved
            } else {
                Outcome::Declined
            };
            self.scenario.error_id = None;
        }
        self.scenario.amount = None;
        self.scenario.id = if self.outcome >= 2 {
            ERRORS[self.outcome - 2].id.to_owned()
        } else if self.outcome == 1 {
            "declined".into()
        } else {
            "approved".into()
        };
        if standalone {
            self.api.send(
                "standalone",
                json!({"scenario":self.scenario,"amount":self.amount}),
            );
        } else {
            self.api.send("arm", json!(self.scenario));
        }
    }
    fn button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                f(this, window, cx);
                cx.notify();
            }))
    }
}
fn display_state(
    op: Option<&paylink_core::Operation>,
    input_amount: u64,
    editing: bool,
) -> (String, u64) {
    match op {
        Some(op) if !op.stage.terminal() || !editing => (format!("{:?}", op.stage), op.amount),
        Some(_) => ("Enter amount".into(), input_amount),
        None => ("Ready".into(), input_amount),
    }
}
impl Render for Terminal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.api.snapshot.lock().unwrap().clone();
        let op = self.current();
        let id = op.as_ref().map(|o| o.id.clone());
        if id != self.last_operation_id {
            self.entry_dirty = id.is_none();
            self.last_operation_id = id;
        }
        let active = op.as_ref().is_some_and(|o| !o.stage.terminal());
        let (stage, amount) = display_state(op.as_ref(), self.amount, self.entry_dirty);
        let detail = op
            .as_ref()
            .filter(|_| !self.entry_dirty || active)
            .and_then(|o| o.error_id.as_deref())
            .and_then(paylink_core::catalog::error_definition)
            .map(|e| e.message.to_owned())
            .unwrap_or_default();
        let elapsed = op
            .as_ref()
            .and_then(|o| {
                snapshot.engine.as_ref().map(|e| {
                    o.completed_ms
                        .unwrap_or(e.now_ms)
                        .saturating_sub(o.started_ms)
                })
            })
            .unwrap_or(0);
        let outcome = match self.outcome {
            0 => "Approved",
            1 => "Declined",
            n => ERRORS[n - 2].id,
        };
        let mut keypad = div().v_flex().gap_2();
        for (row, keys) in [
            ["1", "2", "3"],
            ["4", "5", "6"],
            ["7", "8", "9"],
            ["clear", "0", "backspace"],
        ]
        .iter()
        .enumerate()
        {
            let mut line = div().h_flex().gap_2();
            for (column, key) in keys.iter().enumerate() {
                let key = *key;
                line = line.child(
                    Button::new(("key", row * 3 + column))
                        .label(match key {
                            "clear" => "C",
                            "backspace" => "←",
                            _ => key,
                        })
                        .disabled(active && matches!(self.input, InputTarget::Amount))
                        .w(px(80.))
                        .h(px(44.))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.digit(key);
                            cx.notify();
                        })),
                );
            }
            keypad = keypad.child(line);
        }
        let display = div()
            .id("terminal-display")
            .role(Role::Status)
            .aria_label(format!(
                "Terminal {stage}, amount {}.{:02} UAH, {detail}",
                amount / 100,
                amount % 100
            ))
            .v_flex()
            .gap_2()
            .p_5()
            .rounded_lg()
            .bg(rgb(0xd9efdc))
            .text_color(rgb(0x173d2c))
            .w_full()
            .min_h(px(150.))
            .child(div().text_sm().child("PAYLINK 2.1.20 · SIMULATED"))
            .child(
                div()
                    .text_3xl()
                    .child(format!("{}.{:02} UAH", amount / 100, amount % 100)),
            )
            .child(stage.clone())
            .child(div().text_sm().child(detail))
            .child(div().text_xs().child(format!("Elapsed: {elapsed} ms")))
            .child(
                div().text_xs().child(
                    op.as_ref()
                        .map(|o| o.id.clone())
                        .unwrap_or("Enter amount, then Start".into()),
                ),
            );
        let terminal = div()
            .v_flex()
            .gap_4()
            .p_5()
            .w(px(320.))
            .rounded_3xl()
            .bg(rgb(0x24323e))
            .text_color(rgb(0xf1f5f9))
            .shadow_lg()
            .child(div().text_sm().child("VIRTUAL PAYMENT TERMINAL"))
            .child(div().h(px(6.)).w_full().bg(rgb(0x101820)).rounded_full())
            .child(display)
            .child(keypad)
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        self.button(
                            "cancel",
                            "Cancel",
                            |s, _, _| s.event("customer_cancelled"),
                            cx,
                        )
                        .danger(),
                    )
                    .child(
                        self.button(
                            "ok",
                            "OK",
                            |s, _, _| match s.current().map(|o| o.stage) {
                                Some(paylink_core::Stage::AwaitingCustomer) => {
                                    s.event("customer_confirmed")
                                }
                                Some(paylink_core::Stage::AwaitingConfirmation) => {
                                    s.event("terminal_confirmed")
                                }
                                _ => {}
                            },
                            cx,
                        )
                        .primary(),
                    ),
            )
            .child(self.button(
                "card",
                "Tap / insert test card",
                |s, _, _| s.event("card_presented"),
                cx,
            ))
            .child(div().h(px(8.)).w_full().bg(rgb(0x101820)).rounded_sm())
            .child(
                div()
                    .text_xs()
                    .child("No real cards or PINs. Original generic terminal skin."),
            );
        let mut settings = div().v_flex().gap_3().flex_1().min_w(px(390.));
        settings = settings
            .child(div().text_xl().child("Scenario controls"))
            .child(div().text_sm().child(format!(
                "{} · {}",
                self.base,
                if snapshot.connected {
                    "connected"
                } else {
                    "disconnected"
                }
            )))
            .child(
                div()
                    .id("control-feedback")
                    .role(Role::Status)
                    .aria_label(snapshot.message.clone())
                    .text_sm()
                    .child(snapshot.message.clone()),
            )
            .child(div().text_sm().child(format!(
                "Queued scenarios: {}",
                snapshot.engine.as_ref().map(|e| e.queue.len()).unwrap_or(0)
            )))
            .child(self.button(
                "device",
                format!("Device: {}", self.scenario.device_id),
                |s, _, _| {
                    if let Some(engine) = &s.api.snapshot.lock().unwrap().engine {
                        let devices = engine.devices.keys().cloned().collect::<Vec<_>>();
                        if !devices.is_empty() {
                            s.device_index = (s.device_index + 1) % devices.len();
                            s.scenario.device_id = devices[s.device_index].clone();
                        }
                    }
                },
                cx,
            ))
            .child(self.button(
                "mode",
                format!("Mode: {:?}", self.scenario.mode),
                |s, _, _| {
                    s.scenario.mode = if s.scenario.mode == Mode::Manual {
                        Mode::Automatic
                    } else {
                        Mode::Manual
                    }
                },
                cx,
            ))
            .child(self.button(
                "outcome",
                format!("Next outcome: {outcome}  →"),
                |s, _, _| s.outcome = (s.outcome + 1) % (ERRORS.len() + 2),
                cx,
            ))
            .child(self.button(
                "manual-bank",
                format!("Manual bank decision: {}", self.scenario.manual_bank),
                |s, _, _| s.scenario.manual_bank = !s.scenario.manual_bank,
                cx,
            ))
            .child(
                div()
                    .text_sm()
                    .child("Select a field, then use the keypad (or keyboard digits)."),
            );
        settings = settings
            .child(self.button(
                "delivery",
                format!("Reply fault: {:?} →", self.scenario.delivery),
                |s, _, _| {
                    let choices = [
                        Delivery::Normal,
                        Delivery::DisconnectBeforeAccept,
                        Delivery::DisconnectAfterAccept,
                        Delivery::DisconnectAfterCommit,
                        Delivery::PartialResponse,
                        Delivery::Hang,
                        Delivery::MalformedJson,
                        Delivery::Http400,
                        Delivery::Http500,
                        Delivery::WrongContentType,
                        Delivery::MissingFields,
                        Delivery::UnknownCode,
                    ];
                    let index = choices
                        .iter()
                        .position(|d| *d == s.scenario.delivery)
                        .unwrap_or(0);
                    s.scenario.delivery = choices[(index + 1) % choices.len()];
                },
                cx,
            ))
            .child(self.button(
                "failure-stage",
                format!("Inject error at: {:?} →", self.scenario.failure_stage),
                |s, _, _| {
                    let stages = [
                        Stage::Connecting,
                        Stage::AwaitingCard,
                        Stage::AwaitingCustomer,
                        Stage::Authorizing,
                    ];
                    let index = stages
                        .iter()
                        .position(|v| *v == s.scenario.failure_stage)
                        .unwrap_or(0);
                    s.scenario.failure_stage = stages[(index + 1) % stages.len()];
                },
                cx,
            ));
        for (index, (target, label, value)) in [
            (InputTarget::Amount, "Amount / minor units", self.amount),
            (
                InputTarget::Connect,
                "Connection / ms",
                self.scenario.timing.connect_ms,
            ),
            (
                InputTarget::Card,
                "Automatic card / ms",
                self.scenario.timing.card_ms,
            ),
            (
                InputTarget::Customer,
                "Automatic customer / ms",
                self.scenario.timing.customer_ms,
            ),
            (
                InputTarget::Authorize,
                "Bank processing / ms",
                self.scenario.timing.authorize_ms,
            ),
            (
                InputTarget::Response,
                "Response delivery / ms",
                self.scenario.timing.response_ms,
            ),
            (
                InputTarget::Timeout,
                "Operation timeout / ms",
                self.scenario.timing.timeout_ms,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            settings = settings.child(
                Button::new(("field", index))
                    .label(format!("{label}: {value}"))
                    .disabled(active && matches!(target, InputTarget::Amount))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.input = target;
                        s.replace_input = true;
                        cx.notify();
                    })),
            );
        }
        settings = settings
            .child(
                div()
                    .text_sm()
                    .child(format!("Editing: {}", self.input.label())),
            )
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        self.button("arm", "Arm next request", |s, _, _| s.queue(false), cx)
                            .primary(),
                    )
                    .child(self.button("start", "Start standalone", |s, _, _| s.queue(true), cx)),
            )
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(self.button(
                        "approve",
                        "Bank approves",
                        |s, _, _| s.event("bank_approved"),
                        cx,
                    ))
                    .child(self.button(
                        "decline",
                        "Bank declines",
                        |s, _, _| s.event("bank_declined"),
                        cx,
                    )),
            )
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(self.button(
                        "disconnect",
                        "Disconnect device",
                        |s, _, _| s.event("device_disconnected"),
                        cx,
                    ))
                    .child(self.button(
                        "restore",
                        "Restore device / port",
                        |s, _, _| {
                            s.api.send("transport", json!({"online":true}));
                            s.api.send(
                                "devices",
                                json!(Device {
                                    id: s.scenario.device_id.clone(),
                                    online: true,
                                    setup_error: None,
                                    ..s.configured_device()
                                }),
                            );
                        },
                        cx,
                    )),
            )
            .child(self.button(
                "reset",
                "Reset session and journal",
                |s, _, _| s.api.send("reset", json!({})),
                cx,
            ));
        let mut journal = div()
            .v_flex()
            .gap_1()
            .p_4()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg();
        if let Some(engine) = snapshot.engine {
            journal = journal.child(format!(
                "Requests {} · Accepted {} · Approvals {} · Delivered {}",
                engine.counters.requests,
                engine.counters.accepted,
                engine.counters.approvals,
                engine.counters.delivered
            ));
            for event in engine.events.iter().rev().take(12) {
                journal = journal.child(div().text_xs().child(format!(
                    "#{} · {} ms · {} · {}",
                    event.cursor, event.at_ms, event.kind, event.detail
                )));
            }
        }
        div().id("terminal-app").track_focus(&self.focus).on_key_down(cx.listener(|s,event:&KeyDownEvent,_,cx|{let key=event.keystroke.key.as_str();if key.len()==1&&key.as_bytes()[0].is_ascii_digit()||key=="backspace"{s.digit(key);cx.notify();}}))
            .size_full().overflow_y_scroll().v_flex().gap_5().p_6().bg(cx.theme().background).text_color(cx.theme().foreground)
            .child(div().text_2xl().child("PayLink terminal lab"))
            .child(div().text_sm().child("Experimental 2.1.20 profile · wire compatibility has not been verified against a real terminal"))
            .child(div().h_flex().items_start().gap_6().child(terminal).child(settings)).child(journal)
    }
}
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let runtime = tokio::runtime::Runtime::new()?;
    anyhow::ensure!(
        args.connect.is_none() || args.token.is_some(),
        "--connect requires --token or PAYLINK_CONTROL_TOKEN"
    );
    let token = args
        .token
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let server = if args.connect.is_none() {
        Some(runtime.block_on(Server::start(Config {
            payment_addr: args.payment_addr,
            control_addr: args.control_addr,
            token: token.clone(),
            controlled_clock: false,
            allowed_origins: args.allow_origin,
            journal: args.journal,
        }))?)
    } else {
        None
    };
    let base = args
        .connect
        .unwrap_or_else(|| server.as_ref().unwrap().ready.control_url.clone());
    if let Some(server) = &server {
        println!("{}", serde_json::to_string(&server.ready)?);
    }
    let api = Api::new(base.clone(), token);
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("PayLink Emulator".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(830.), px(650.))),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(80.), px(60.)),
                        size(px(1000.), px(980.)),
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| Terminal::new(api, base, cx));
                    let focus = view.read(cx).focus.clone();
                    focus.focus(window, cx);
                    cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
                },
            )
            .expect("open terminal window");
        })
        .detach();
        cx.activate(true);
    });
    if let Some(server) = server {
        runtime.block_on(server.shutdown())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::display_state;
    use paylink_core::{DEVICE_ID, Engine, Scenario};
    #[test]
    fn browser_payment_amount_is_preserved_after_completion_until_new_entry() {
        let mut engine = Engine::default();
        engine.arm(Scenario::default()).unwrap();
        let id = engine.start(DEVICE_ID, 2600, None).unwrap();
        assert_eq!(
            display_state(Some(&engine.operations[&id]), 100, true).1,
            2600
        );
        engine.advance(5000).unwrap();
        assert_eq!(
            display_state(Some(&engine.operations[&id]), 100, false),
            ("Approved".into(), 2600)
        );
        assert_eq!(
            display_state(Some(&engine.operations[&id]), 999, true),
            ("Enter amount".into(), 999)
        );
    }
}
