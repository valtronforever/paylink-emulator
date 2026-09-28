fn main() {
    let schemas = serde_json::json!({
        "Scenario":schemars::schema_for!(paylink_core::Scenario),
        "Device":schemars::schema_for!(paylink_core::Device),
        "Engine":schemars::schema_for!(paylink_core::Engine),
    });
    println!("{}", serde_json::to_string_pretty(&schemas).unwrap());
}
