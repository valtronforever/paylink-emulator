use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct ErrorDefinition {
    pub id: &'static str,
    pub message: &'static str,
    pub category: &'static str,
    pub wire_status: &'static str,
    pub source: &'static str,
}
const SOURCE: &str = "https://wiki.checkbox.ua/app/pc/desktop_paylink_errors";
macro_rules! error {
    ($id:literal, $message:literal, $category:literal) => {
        ErrorDefinition {
            id: $id,
            message: $message,
            category: $category,
            wire_status: "unverified",
            source: SOURCE,
        }
    };
}
pub const ERRORS: &[ErrorDefinition] = &[
    error!("paylink_not_running", "PayLink is not running", "transport"),
    error!(
        "terminal_unknown_0",
        "ErrorCode=0 : Unknown error", "terminal"
    ),
    error!("terminal_general", "General error", "terminal"),
    error!(
        "merchant_profile_invalid",
        "ОПЛАТА: ПОМИЛКОВИЙ ПРОФІЛЬ!", "terminal"
    ),
    error!("host_clock_invalid", "НЕВІДОМА ПОМИЛКА rr", "terminal"),
    error!(
        "bank_host_unreachable",
        "No connection with host", "terminal"
    ),
    error!(
        "customer_cancelled",
        "Transaction canceled by user", "terminal"
    ),
    error!(
        "terminal_connection_refused",
        "Connection refused", "terminal"
    ),
    error!("terminal_busy", "Device is busy", "terminal"),
    error!("terminal_timeout", "Timed out", "terminal"),
    error!(
        "terminal_id_invalid",
        "Invalid terminal identifier", "terminal"
    ),
    error!("verification_fault", "Verification fault", "terminal"),
    error!(
        "driver_install_9011",
        "Driver installation error 9011", "setup_only"
    ),
];
pub fn error_definition(id: &str) -> Option<&'static ErrorDefinition> {
    ERRORS.iter().find(|item| item.id == id)
}
