# PayLink emulator

[Українська документація](README.uk.md)

A Rust terminal simulator for manual testing and browser automation, targeting **Checkbox Desktop PayLink 2.1.20 win-x86**. Run a headless local service in CI, control it with an API/CLI, or use a native GPUI terminal with a display, keypad and card/customer actions.

**Documentation-based contract:** the profile targets PayLink 2.1.20 using the published setup/error documentation and the pinned build's API contract evidence. Physical terminal recordings are optional; they are not an acceptance gate. Defaults for undocumented behavior are listed in [the contract and assumptions](docs/DOCUMENTATION_CONTRACT.md). Reference compatibility remains unverified; the emulator cannot contact a bank or charge a real card.

## Quick start

Install [Rust](https://rustup.rs/); `rust-toolchain.toml` pins the compiler. Linux/macOS/Windows use the same CLI. Commands below use POSIX shell syntax; in PowerShell set `$env:PAYLINK_CONTROL_TOKEN = 'local-test-token-at-least-16-chars'`.

```sh
cargo build --locked -p paylink-emulator
export PAYLINK_CONTROL_TOKEN='local-test-token-at-least-16-chars'
cargo run --locked -p paylink-emulator -- serve \
  --allow-origin https://your-test-ui.example \
  --journal ./journal.json
```

Readiness is one JSON line with actual payment/control URLs and profile. Both listeners bind loopback; defaults are ports 3000 and 3001. Use `--payment-addr 127.0.0.1:0 --control-addr 127.0.0.1:0` for isolated dynamic ports. Each worker should own its process, token, ports and journal. There is no passthrough mode.

The payment adapter currently accepts `amount` and optional `merchant_id`. Unsupported fields, including real PayLink transaction IDs and confirmation options, return HTTP 501 without starting payment. See the compatibility guide before writing integration tests.

In another terminal with the same token:

```sh
cargo run -p paylink-emulator -- arm --state approved --authorize-ms 1500
# Now let your browser client POST {"amount":100} to:
# http://127.0.0.1:3000/api/pos/00000000-0000-4000-8000-000000000001/purchase
cargo run -p paylink-emulator -- get state
cargo run -p paylink-emulator -- assert '{"approvals":1,"queue_empty":true}'
cargo run -p paylink-emulator -- export ./result.json
```

No queued scenario means a test failure, never an implicit approval. Money is positive integer minor units (100 = 1.00 UAH). A repeated purchase can cause another simulated charge; the emulator deliberately does not invent payment idempotency.

## Native terminal

```sh
cargo run --locked -p paylink-gui
# Or attach to a CLI-launched service:
cargo run --locked -p paylink-gui -- --connect http://127.0.0.1:3001
```

The native app starts the same service when `--connect` is absent. Its original generic terminal skin has a display, physical-style keypad, Cancel/OK and a test-card button. Choose a field and use the keypad to enter the amount or a delay. **Arm next request** prepares a browser-driven payment; **Start standalone** lets you practice the entire flow without another app. In manual mode, card and customer stages wait for clicks. Enable manual bank decisions to hold authorization until you approve or decline. All 13 documented common cases are selectable; transport/setup cases use their own controls. The journal shows stage changes and separate charge/delivery counts.

A default embedded token is randomly generated; set `PAYLINK_CONTROL_TOKEN` if a CLI/test runner must also control that session. An attached service requires its existing token. Native UI needs a graphics session and platform development libraries. Headless `cargo build`/`cargo test` use the workspace default members and do not compile GPUI.

## Scenarios

```sh
cargo run -p paylink-emulator -- arm --state error --error-id terminal_busy
cargo run -p paylink-emulator -- arm --state manual
cargo run -p paylink-emulator -- arm --file examples/lost-reply.json
cargo run -p paylink-emulator -- get operations
cargo run -p paylink-emulator -- action g1-op1 card_presented
cargo run -p paylink-emulator -- action g1-op1 customer_confirmed
cargo run -p paylink-emulator -- command transport '{"online":false}'
cargo run -p paylink-emulator -- reset
```

[Scenario examples](examples/) use the full model defaults for omitted fields. Automatic stages have configurable connection/card/customer/bank/confirmation/response delays. Manual card/customer/bank events hold only their stage; monotonic deadlines still run. An operation snapshots its scenario so later configuration cannot rewrite an in-flight result.

Use `serve --clock controlled` then `advance 100` for deterministic millisecond boundaries. Browser/realtime tests should use the default clock. `--delivery disconnect_after_commit` loses the reply after approval. Other policies include disconnect before acceptance, partial response, malformed JSON, HTTP 500 and an indefinite reply hang. These are synthetic fault injections, not claims about documented PayLink codes.

## API and tests

- [Control API](docs/CONTROL_API.md) / [OpenAPI 3.1](docs/control-openapi.json): token-authenticated runner/CLI/native control, separate from the payment listener.
- [Compatibility manifest](profiles/desktop-paylink-2.1.20-win-x86/manifest.json): exact version, evidence and unsupported routes.
- [Testing and coverage](docs/TESTING.md): native, HTTP and actual browser tests; reference calibration boundary.
- [Headless container](docs/CONTAINERS.md): isolated network and runtime instructions.
- [Project issues](https://github.com/valtronforever/paylink-emulator/issues): implementation and remaining verification work.

```sh
cargo test --locked
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
npm ci
npx playwright install chromium
cargo build --locked -p paylink-emulator
npm run test:browser
```

Playwright uses a temporary self-signed HTTPS harness (OpenSSL required), actual localhost connections and no request interception. Artifacts go to `test-results/`. Control credentials never enter the browser page. Closing/reloading that page does not clear server state. `reset` invalidates old operations and changes the generation.

## Architecture

`paylink-core` is a deterministic state machine with no runtime or graphics dependency. `paylink-server` owns the clock, local listeners and control API. The payment listener can be stopped without losing control/journal access. `paylink-emulator` is the clap CLI; `paylink-gui` uses GPUI Kit and sends the same control commands as tests. GUI network I/O runs off the rendering thread.

GPUI Kit is pinned to 0.6.4 and its GPUI pre-release family to 0.3.5 because 0.3.6 changes the inspector callback API. Keep `Cargo.lock`; update the family together and run native builds before upgrading.

Official references: [connection/release notes](https://wiki.checkbox.ua/app/pc/portal_acquiring), [common errors](https://wiki.checkbox.ua/app/pc/desktop_paylink_errors), [2.1.20 installer](https://api.checkbox.ua/update-service/api/v1/pos_srv/versions/2.1.20/win-x86/installer).

MIT license. Original UI artwork is generic and does not imply endorsement by Checkbox or a terminal manufacturer.
