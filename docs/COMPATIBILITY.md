# Compatibility evidence

Target: **Checkbox Desktop PayLink 2.1.20, win-x86**. Emulator host platforms are independent of this target.

The [manifest](../profiles/desktop-paylink-2.1.20-win-x86/manifest.json) records the official installer URL and SHA-256 actually computed on 2026-09-21. The executable has **not** been run against a physical terminal. API bodies, error channels, numeric codes, model-specific phases, timeout boundaries and browser headers remain **unverified**. Passing emulator tests proves its own model behaves consistently, not compatibility with a bank.

The installer contains `POSServer.xml`. Its `Purchase` documentation shows an integer `amount` body and 200/400 responses. `GetDeviceConfig` documents 200/404; registration requires a subsequent `/api/pos/saveconfig`. It also lists ping, merchant discovery, last operation, report and WebSocket methods. Those names alone are not enough to invent paths or DTOs. Only the four routes in the manifest are provisionally implemented; unsupported routes return 404 and are not advertised as compatible.

The provisional `success/error/code/result` wrapper and purchase result field names were cross-checked against [this community SDK](https://github.com/MakarovIgor/checkbox.paylink.php.sdk). This is secondary evidence, not proof of 2.1.20 behavior. In particular, `code: 0`, success details and device JSON must be replaced or confirmed through reference fixtures.

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
