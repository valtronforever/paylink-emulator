# Compatibility evidence

Target: **Checkbox Desktop PayLink 2.1.20, win-x86**. Emulator host platforms are independent of this target.

The [manifest](../profiles/desktop-paylink-2.1.20-win-x86/manifest.json) records the official installer URL and SHA-256 actually computed on 2026-09-21. The executable has **not** been run against a physical terminal. API bodies, error channels, numeric codes, model-specific phases, timeout boundaries and browser headers remain **unverified**. Passing emulator tests proves its own model behaves consistently, not compatibility with a bank.

## Static evidence from the pinned build

The installer contains `POSServer.exe` (assembly version **2.1.20.10**) and `POSServer.xml`. [The metadata snapshot](../profiles/desktop-paylink-2.1.20-win-x86/static-contract.json) records 169 native status constants, 51 HTTP route attributes and 96 DTO properties with serializer attributes. It contains public contract metadata only, not the application implementation, certificates or keys.

The native status enum includes progress and success values as well as errors: 169 does **not** mean 169 injectible payment errors. `ServiceCommand.GetErrorsList` enumerates all of them. Their availability, error channel, bank mapping and timing cannot be inferred just from the enum. The emulator's 13 common scenarios are a separate, smaller coverage set.

Statically established facts applied to this draft:

- The purchase DTO uses unsigned integer `amount` and string `merchant_id`. The model/control API still calls its configuration field `merchant`.
- Discovery supports both `/api/devices` and `/api/pos/devices`, including the device ID variants. A missing configuration returns HTTP 404 with `loc`, `msg`, and `type` fields.
- Ping of an unknown device returns HTTP 404 and native code 9524 (`InvalidTerminalId`). A concurrent purchase/ping against a locked device returns HTTP 400 and code 9009 (`DeviceBusy`). These are distinct from an injected terminal-error scenario.
- `BaseResponseDTO` serializes `terminal_status` as a string and initializes `error` to an empty string. A purchase response carries an operation `id`. The provisional success body now follows these rules.
- In `ResponseDTO`, `terminal` aliases `terminal_id`, `value` aliases `amount`, and `receipt_no` is the string form of numeric `invoice_num`. The test body no longer invents `card_name`, nested `code` or `commission` fields.

Response details still form a **synthetic subset**, not a full bank-specific DTO. In particular discovery data, terminal failure payloads and many success fields still require reference calibration. A static identifier does not establish how a provider uses it. The earlier [community SDK](https://github.com/MakarovIgor/checkbox.paylink.php.sdk) was useful for initial discovery but is superseded by the version-specific metadata where they differ.

The supported purchase body is currently `{"amount":100,"merchant_id":"TEST-MERCHANT"}` with `merchant_id` optional. Other fields return explicit HTTP 501 before an operation starts. Real PayLink has request-ID/deduplication, signature/continuation and further options; silently ignoring them would give tests misleading duplicate-payment behavior. HTTP 501 for these fields is an **emulator limitation**, not asserted PayLink behavior. Calls without IDs remain independent operations.

To reproduce the metadata snapshot, extract the pinned NSIS installer into a temporary directory, install `dnfile==0.18.0` in an isolated Python environment, and run:

```sh
python scripts/extract-static-contract.py /path/to/POSServer.exe output.json
```

The script refuses a different assembly hash. JSON serializer options are stored as numeric enum values; for example `NullValueHandling: 1` means omit null values. Route attributes retain `[controller]`; the controller prefix is `api/[controller]` and its name is `POS`. The snapshot inventories additional endpoints but does not claim they are implemented.

## Error taxonomy

[Checkbox's common error page](https://wiki.checkbox.ua/app/pc/desktop_paylink_errors) describes 13 cases. Stable emulator `error_id` values are our identifiers, not PayLink numeric codes. `GET /control/v1/errors` exposes the complete current catalog, including evidence status and source URL.

Eleven terminal errors can be selected with `outcome=error` and `error_id`. `paylink_not_running` is a real unbound payment listener through the `transport` control command. `driver_install_9011` is a setup-only device fault: configure `setup_error`, which makes ping/purchase unavailable. This consequence is synthetic pending reference confirmation; it does not claim to reproduce driver installation or show code 9011 to the payment browser.

Confirmation/reversal state-machine support is **synthetic and opt-in**, never enabled by the default PayLink profile. It models tests of uncertain outcomes but must not imply that any bank requires that handshake. Likewise, the default 120-second timeout is configurable test data until measured.

## Reference calibration procedure

1. On an isolated Windows test machine, download the manifest URL and verify the SHA-256 before installing. Record Windows build, PayLink build information, terminal model, bank and configured protocol.
2. Export the local API documentation and record exact methods, paths, headers, DTOs and response status/body. Do not use the Checkbox fiscal API schema as a substitute.
3. Capture safe test-mode discovery, ping, approved/declined and each supported error scenario. Record relative monotonic timings, customer actions, terminal state and bank result separately from response delivery. Never use production cards or copy PINs, tokens or unmasked card data.
4. Normalize only genuinely variable reference fields (IDs/timestamps). Keep types, nullability, numeric codes and error channel intact. Compare the same request against the emulator.
5. Check actual CORS/preflight and local-network permission behavior using the supported browsers. Do not disable browser security to make a test pass.
6. Add sanitized fixtures under the version profile, link every catalog entry to its fixture and test, and change individual coverage status only with supporting evidence. No fixture means `unverified/blocked`, not a skipped pass.
7. Separate bank-specific timing/confirmation rules into named profiles. Never silently move the 2.1.20 profile to a newer PayLink version.

Source setup/release documentation: https://wiki.checkbox.ua/app/pc/portal_acquiring

The original Inerix issue is retained as historical context in `ORIGINAL_REQUIREMENTS.md`. The user's later decision moved the emulator to its own Rust repository and added a native GPUI interface; those decisions supersede its old path suggestions and GUI exclusion.

## Differential runner

`node scripts/differential.mjs` compares sanitized reference fixtures with a **running emulator**. It does not record payments against a real bank. Without fixtures it writes `test-results/differential.json` with `blocked` and exits 2. This is the expected current result, never a passed calibration.

Place reference JSON files in `profiles/desktop-paylink-2.1.20-win-x86/reference/`. Each contains:

```json
{
  "profile": "desktop-paylink-2.1.20-win-x86",
  "evidence": {
    "installer_sha256": "62d0e7a539380937ecb5af2f1c50438e4b84a070de4a20c1c848c11a5e395491",
    "bank": "record the actual test bank",
    "protocol": "record the actual terminal protocol",
    "capture_date": "YYYY-MM-DD"
  },
  "request": {"method": "GET", "path": "/api/devices/", "headers": {}},
  "response": {"status": 200, "body": []},
  "variable_fields": []
}
```

This is a **format example**, not an actual reference response. Payment fixtures additionally carry a matching emulator `scenario` and JSON request `body`. Optional `timing` has `min_ms` and `max_ms`. Only operation ID and result RRN, authorization code, invoice number and receipt number may be explicitly normalized; absence/types/status/error content remain significant. Set `PAYLINK_PAYMENT_URL`, `PAYLINK_CONTROL_URL`, and `PAYLINK_CONTROL_TOKEN` to the isolated emulator. The runner resets it before every case.
