//! Opt-in mapping observed in POSServer 2.1.20.10 against simulated SSI JSON.
//! These are response facts, not a universal bank error taxonomy.
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

pub fn error_response(case: &str) -> Option<Value> {
    static RESPONSES: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    RESPONSES
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../profiles/desktop-paylink-2.1.20-win-x86/ssi/http-errors.json"
            ))
            .expect("checked-in SSI error mappings must be valid JSON")
        })
        .get(case)
        .cloned()
}
