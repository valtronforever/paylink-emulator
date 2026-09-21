fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(paylink_core::catalog::ERRORS).unwrap()
    );
}
