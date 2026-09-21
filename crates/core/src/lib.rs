//! Deterministic terminal model. It has no network, UI or wall-clock dependency.
pub mod catalog;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use thiserror::Error;

pub const PROFILE: &str = "desktop-paylink-2.1.20-win-x86";
pub const DEVICE_ID: &str = "00000000-0000-4000-8000-000000000001";

#[derive(Debug, Error, Clone, Serialize, JsonSchema)]
#[error("{code}: {message}")]
pub struct ModelError {
    pub code: String,
    pub message: String,
}
fn fail(code: &str, message: &str) -> ModelError {
    ModelError {
        code: code.into(),
        message: message.into(),
    }
}
type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Connecting,
    AwaitingCard,
    AwaitingCustomer,
    Authorizing,
    AwaitingConfirmation,
    Approved,
    Declined,
    Cancelled,
    Failed,
    TimedOut,
    Reversed,
}
impl Stage {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Approved
                | Self::Declined
                | Self::Cancelled
                | Self::Failed
                | Self::TimedOut
                | Self::Reversed
        )
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Automatic,
    Manual,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    #[default]
    Approved,
    Declined,
    Error,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    #[default]
    Normal,
    DisconnectBeforeAccept,
    DisconnectAfterCommit,
    DisconnectAfterAccept,
    PartialResponse,
    Hang,
    MalformedJson,
    Http500,
    Http400,
    WrongContentType,
    MissingFields,
    UnknownCode,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Timing {
    pub connect_ms: u64,
    pub card_ms: u64,
    pub customer_ms: u64,
    pub authorize_ms: u64,
    pub confirm_ms: u64,
    pub response_ms: u64,
    pub timeout_ms: u64,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            connect_ms: 300,
            card_ms: 2000,
            customer_ms: 500,
            authorize_ms: 1200,
            confirm_ms: 500,
            response_ms: 0,
            timeout_ms: 120_000,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub device_id: String,
    pub amount: Option<u64>,
    pub merchant: Option<String>,
    pub uses: u32,
    pub mode: Mode,
    pub outcome: Outcome,
    pub error_id: Option<String>,
    pub failure_stage: Stage,
    pub timing: Timing,
    pub delivery: Delivery,
    pub require_confirmation: bool,
    pub manual_bank: bool,
    pub seed: u64,
}
impl Default for Scenario {
    fn default() -> Self {
        Self {
            id: "approved".into(),
            device_id: DEVICE_ID.into(),
            amount: None,
            merchant: None,
            uses: 1,
            mode: Mode::Automatic,
            outcome: Outcome::Approved,
            error_id: None,
            failure_stage: Stage::Authorizing,
            timing: Timing::default(),
            delivery: Delivery::Normal,
            require_confirmation: false,
            manual_bank: false,
            seed: 42,
        }
    }
}
impl Scenario {
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() || self.id.len() > 128 || self.uses == 0 || self.uses > 10_000 {
            return Err(fail(
                "invalid_scenario",
                "id and uses (1..10000) are required",
            ));
        }
        if self.timing.timeout_ms == 0 || self.timing.timeout_ms > 3_600_000 {
            return Err(fail("invalid_timing", "timeout must be 1..3600000 ms"));
        }
        for value in [
            self.timing.connect_ms,
            self.timing.card_ms,
            self.timing.customer_ms,
            self.timing.authorize_ms,
            self.timing.confirm_ms,
            self.timing.response_ms,
        ] {
            if value > 3_600_000 {
                return Err(fail("invalid_timing", "stage delay exceeds one hour"));
            }
        }
        if self.failure_stage.terminal() {
            return Err(fail(
                "invalid_stage",
                "failure must occur during an active stage",
            ));
        }
        if self.failure_stage == Stage::AwaitingConfirmation && !self.require_confirmation {
            return Err(fail("invalid_stage", "confirmation stage is disabled"));
        }
        match (&self.error_id, self.outcome) {
            (Some(id), Outcome::Error) => {
                let error = catalog::error_definition(id)
                    .ok_or_else(|| fail("unknown_error", "unknown error_id"))?;
                if error.category != "terminal" {
                    return Err(fail(
                        "not_payment_error",
                        "configure transport/device availability for transport/setup errors",
                    ));
                }
            }
            (None, Outcome::Error) | (Some(_), _) => {
                return Err(fail(
                    "invalid_outcome",
                    "error outcome requires exactly one error_id",
                ));
            }
            _ => {}
        }
        if self.amount == Some(0) {
            return Err(fail(
                "invalid_amount",
                "amount must be positive minor units",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub merchant: String,
    pub online: bool,
    #[serde(default)]
    pub setup_error: Option<String>,
}
impl Default for Device {
    fn default() -> Self {
        Self {
            id: DEVICE_ID.into(),
            name: "Virtual POS".into(),
            merchant: "TEST-MERCHANT".into(),
            online: true,
            setup_error: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Operation {
    pub id: String,
    pub device_id: String,
    pub merchant: String,
    pub amount: u64,
    pub stage: Stage,
    pub started_ms: u64,
    pub stage_started_ms: u64,
    pub completed_ms: Option<u64>,
    pub deadline_ms: u64,
    pub charged: bool,
    pub error_id: Option<String>,
    pub scenario: Scenario,
    pub response_delivered: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Counters {
    pub requests: u64,
    pub accepted: u64,
    pub approvals: u64,
    pub reversals: u64,
    pub delivered: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub cursor: u64,
    pub generation: u64,
    pub at_ms: u64,
    pub unix_ms: Option<u64>,
    pub profile: String,
    pub device_id: Option<String>,
    pub scenario_id: Option<String>,
    pub charged: Option<bool>,
    pub operation_id: Option<String>,
    pub kind: String,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Engine {
    pub profile: String,
    pub now_ms: u64,
    pub epoch_unix_ms: Option<u64>,
    pub generation: u64,
    pub devices: BTreeMap<String, Device>,
    pub operations: BTreeMap<String, Operation>,
    pub queue: VecDeque<Scenario>,
    pub events: Vec<Event>,
    pub counters: Counters,
    next_operation: u64,
    next_event: u64,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            profile: PROFILE.into(),
            now_ms: 0,
            epoch_unix_ms: None,
            generation: 1,
            devices: BTreeMap::from([(DEVICE_ID.into(), Device::default())]),
            operations: BTreeMap::new(),
            queue: VecDeque::new(),
            events: Vec::new(),
            counters: Counters::default(),
            next_operation: 1,
            next_event: 1,
        }
    }
}
impl Engine {
    pub fn with_epoch(mut self, epoch_unix_ms: u64) -> Self {
        self.epoch_unix_ms = Some(epoch_unix_ms);
        self
    }
    fn log(&mut self, operation_id: Option<&str>, kind: &str, detail: String) {
        let operation = operation_id.and_then(|id| self.operations.get(id));
        self.events.push(Event {
            cursor: self.next_event,
            generation: self.generation,
            at_ms: self.now_ms,
            unix_ms: self
                .epoch_unix_ms
                .map(|epoch| epoch.saturating_add(self.now_ms)),
            profile: self.profile.clone(),
            device_id: operation.map(|o| o.device_id.clone()),
            scenario_id: operation.map(|o| o.scenario.id.clone()),
            charged: operation.map(|o| o.charged),
            operation_id: operation_id.map(str::to_owned),
            kind: kind.into(),
            detail,
        });
        self.next_event += 1;
    }
    pub fn arm(&mut self, scenario: Scenario) -> Result<()> {
        scenario.validate()?;
        if !self.devices.contains_key(&scenario.device_id) {
            return Err(fail(
                "terminal_id_invalid",
                "scenario device does not exist",
            ));
        }
        self.log(None, "scenario_armed", scenario.id.clone());
        self.queue.push_back(scenario);
        Ok(())
    }
    pub fn set_device(&mut self, device: Device) -> Result<()> {
        if device.id.is_empty() || device.name.is_empty() || device.merchant.is_empty() {
            return Err(fail("invalid_device", "id, name and merchant are required"));
        }
        if device
            .setup_error
            .as_deref()
            .is_some_and(|v| v != "driver_install_9011")
        {
            return Err(fail(
                "invalid_setup_error",
                "only documented setup errors are accepted",
            ));
        }
        if self
            .operations
            .values()
            .any(|op| op.device_id == device.id && !op.stage.terminal())
        {
            return Err(fail(
                "terminal_busy",
                "cannot reconfigure an active terminal",
            ));
        }
        self.log(None, "device_configured", device.id.clone());
        self.devices.insert(device.id.clone(), device);
        Ok(())
    }
    pub fn start(
        &mut self,
        device_id: &str,
        amount: u64,
        merchant: Option<&str>,
    ) -> Result<String> {
        self.counters.requests += 1;
        self.log(
            None,
            "payment_request",
            format!("{device_id} amount={amount}"),
        );
        let device = self
            .devices
            .get(device_id)
            .ok_or_else(|| fail("terminal_id_invalid", "unknown device"))?
            .clone();
        if amount == 0 || amount > 999_999_999 {
            return Err(fail("invalid_amount", "expected 1..999999999 minor units"));
        }
        if !device.online || device.setup_error.is_some() {
            return Err(fail(
                "terminal_connection_refused",
                "terminal unavailable (check setup via control API)",
            ));
        }
        if merchant.is_some_and(|m| m != device.merchant) {
            return Err(fail("merchant_profile_invalid", "merchant mismatch"));
        }
        if self
            .operations
            .values()
            .any(|op| op.device_id == device_id && !op.stage.terminal())
        {
            return Err(fail(
                "terminal_busy",
                "terminal already processing a payment",
            ));
        }
        let index = self
            .queue
            .iter()
            .position(|s| s.device_id == device_id)
            .ok_or_else(|| fail("scenario_missing", "strict mode: arm a scenario first"))?;
        let scenario = self.queue[index].clone();
        if scenario.amount.is_some_and(|a| a != amount)
            || scenario
                .merchant
                .as_deref()
                .is_some_and(|m| m != device.merchant)
        {
            return Err(fail(
                "scenario_mismatch",
                "request does not match queued scenario",
            ));
        }
        if scenario.uses == 1 {
            self.queue.remove(index);
        } else {
            self.queue[index].uses -= 1;
        }
        if scenario.delivery == Delivery::DisconnectBeforeAccept {
            self.log(None, "transport_fault", "disconnect_before_accept".into());
            return Err(fail(
                "disconnect_before_accept",
                "connection closed before terminal accepted command",
            ));
        }
        let id = format!("g{}-op{}", self.generation, self.next_operation);
        self.next_operation += 1;
        self.counters.accepted += 1;
        let op = Operation {
            id: id.clone(),
            device_id: device_id.into(),
            merchant: device.merchant,
            amount,
            stage: Stage::Connecting,
            started_ms: self.now_ms,
            stage_started_ms: self.now_ms,
            completed_ms: None,
            deadline_ms: self.now_ms + scenario.timing.timeout_ms,
            charged: false,
            error_id: None,
            scenario,
            response_delivered: false,
        };
        self.operations.insert(id.clone(), op);
        self.log(Some(&id), "started", "connecting".into());
        self.advance(0)?;
        Ok(id)
    }
    fn transition(&mut self, id: &str, stage: Stage, error: Option<String>) {
        let op = self.operations.get_mut(id).expect("known operation");
        if stage == Stage::Approved && !op.charged {
            op.charged = true;
            self.counters.approvals += 1;
        }
        if stage == Stage::Reversed && op.charged {
            op.charged = false;
            self.counters.reversals += 1;
        }
        op.stage = stage;
        op.stage_started_ms = self.now_ms;
        op.error_id = error;
        if stage.terminal() {
            op.completed_ms = Some(self.now_ms);
        }
        self.log(Some(id), "transition", format!("{stage:?}"));
    }
    fn due(op: &Operation) -> Option<u64> {
        if op.stage.terminal() {
            return None;
        }
        let t = &op.scenario.timing;
        let delay = match op.stage {
            Stage::Connecting => t.connect_ms,
            Stage::AwaitingCard if op.scenario.mode == Mode::Automatic => t.card_ms,
            Stage::AwaitingCustomer if op.scenario.mode == Mode::Automatic => t.customer_ms,
            Stage::Authorizing if !op.scenario.manual_bank => t.authorize_ms,
            Stage::AwaitingConfirmation if op.scenario.mode == Mode::Automatic => t.confirm_ms,
            _ => return Some(op.deadline_ms),
        };
        Some(
            op.stage_started_ms
                .saturating_add(delay)
                .min(op.deadline_ms),
        )
    }
    fn step(&mut self, id: &str) {
        let op = self.operations[id].clone();
        // Timeout wins a tie; provisional authorization is not a final charge.
        if self.now_ms >= op.deadline_ms {
            self.transition(id, Stage::TimedOut, Some("terminal_timeout".into()));
            return;
        }
        if op.scenario.outcome == Outcome::Error && op.stage == op.scenario.failure_stage {
            let error = op.scenario.error_id.clone().unwrap();
            let stage = match error.as_str() {
                "customer_cancelled" => Stage::Cancelled,
                "terminal_timeout" => Stage::TimedOut,
                _ => Stage::Failed,
            };
            self.transition(id, stage, Some(error));
            return;
        }
        let next = match op.stage {
            Stage::Connecting => Stage::AwaitingCard,
            Stage::AwaitingCard => Stage::AwaitingCustomer,
            Stage::AwaitingCustomer => Stage::Authorizing,
            Stage::Authorizing if op.scenario.outcome == Outcome::Declined => Stage::Declined,
            Stage::Authorizing if op.scenario.require_confirmation => Stage::AwaitingConfirmation,
            Stage::Authorizing | Stage::AwaitingConfirmation => Stage::Approved,
            _ => return,
        };
        self.transition(id, next, None);
    }
    pub fn advance(&mut self, delta_ms: u64) -> Result<()> {
        let target = self
            .now_ms
            .checked_add(delta_ms)
            .ok_or_else(|| fail("clock_overflow", "clock overflow"))?;
        // There are at most six scheduled transitions per operation, including zero delays.
        loop {
            let next = self
                .operations
                .values()
                .filter_map(|op| Self::due(op).map(|at| (at, op.id.clone())))
                .filter(|(at, _)| *at <= target)
                .min();
            let Some((at, id)) = next else {
                break;
            };
            self.now_ms = at.max(self.now_ms);
            self.step(&id);
        }
        self.now_ms = target;
        Ok(())
    }
    pub fn action(&mut self, id: &str, event: &str) -> Result<()> {
        let op = self
            .operations
            .get(id)
            .ok_or_else(|| fail("operation_not_found", "unknown operation"))?
            .clone();
        let next = match (op.stage, event) {
            (Stage::AwaitingCard, "card_presented") => Stage::AwaitingCustomer,
            (Stage::AwaitingCustomer, "customer_confirmed") => Stage::Authorizing,
            (Stage::Authorizing, "bank_approved") if op.scenario.manual_bank => {
                if op.scenario.require_confirmation {
                    Stage::AwaitingConfirmation
                } else {
                    Stage::Approved
                }
            }
            (Stage::Authorizing, "bank_declined") if op.scenario.manual_bank => Stage::Declined,
            (Stage::AwaitingConfirmation, "terminal_confirmed") => Stage::Approved,
            (Stage::Approved, "reversed") if op.scenario.require_confirmation => Stage::Reversed,
            (
                Stage::Connecting | Stage::AwaitingCard | Stage::AwaitingCustomer,
                "customer_cancelled",
            ) => Stage::Cancelled,
            (s, "device_disconnected") if !s.terminal() => Stage::Failed,
            _ => {
                return Err(fail(
                    "invalid_transition",
                    "event is not allowed in this stage",
                ));
            }
        };
        let error = match next {
            Stage::Cancelled => Some("customer_cancelled".into()),
            Stage::Failed => Some("terminal_connection_refused".into()),
            _ => None,
        };
        self.log(Some(id), "input_event", event.into());
        if op.scenario.outcome == Outcome::Error
            && op.stage == op.scenario.failure_stage
            && !matches!(event, "customer_cancelled" | "device_disconnected")
        {
            self.step(id);
        } else {
            self.transition(id, next, error);
        }
        self.advance(0)
    }
    pub fn delivered(&mut self, id: &str) -> Result<()> {
        let op = self
            .operations
            .get_mut(id)
            .ok_or_else(|| fail("operation_not_found", "unknown operation"))?;
        if !op.stage.terminal() {
            return Err(fail("not_complete", "operation still active"));
        }
        if !op.response_delivered {
            op.response_delivered = true;
            self.counters.delivered += 1;
            self.log(Some(id), "response_delivered", String::new());
        }
        Ok(())
    }
    pub fn reset(&mut self) {
        let generation = self.generation + 1;
        let epoch_unix_ms = self
            .epoch_unix_ms
            .map(|epoch| epoch.saturating_add(self.now_ms));
        let next_event = self.next_event;
        *self = Self::default();
        self.generation = generation;
        self.epoch_unix_ms = epoch_unix_ms;
        self.next_event = next_event;
        self.log(None, "reset", format!("generation {generation}"));
    }
    pub fn result(&self, id: &str) -> Result<serde_json::Value> {
        let op = self
            .operations
            .get(id)
            .ok_or_else(|| fail("operation_not_found", "unknown operation"))?;
        if !op.stage.terminal() {
            return Err(fail("not_complete", "operation still active"));
        }
        if op.stage == Stage::Approved {
            let sequence = op
                .id
                .split("-op")
                .last()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let serial = op
                .scenario
                .seed
                .wrapping_add(op.started_ms)
                .wrapping_add(op.amount)
                .wrapping_add(sequence);
            Ok(
                serde_json::json!({"success":true,"error":null,"code":0,"result":{
                    "terminal":op.device_id,"rrn":format!("{:012}",serial%1_000_000_000_000),
                    "card_mask":"4444 44** **** 1111","card_name":"TEST CARD","auth_code":format!("{:06}",serial%1_000_000),
                    "payment_system":"VISA","receipt_no":op.id,"acquirer_and_seller":"TEST ACQUIRER / TEST MERCHANT",
                    "amount":op.amount,"code":1,"commission":0
                }}),
            )
        } else {
            let message = op
                .error_id
                .as_deref()
                .and_then(catalog::error_definition)
                .map(|e| e.message)
                .unwrap_or("Payment declined");
            // Wire codes are intentionally not claimed verified until reference capture.
            Ok(serde_json::json!({"success":false,"error":message,"code":0,"result":null}))
        }
    }
}
