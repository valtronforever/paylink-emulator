# Validation and completion audit

Date: 2026-09-21. Target: Desktop PayLink 2.1.20 win-x86.

## Executed locally

| Check | Result | Evidence |
| --- | --- | --- |
| Core state model | 12 passed | `cargo test --locked`, `crates/core/tests/model.rs` |
| CLI command schema and live API workflow | 2 passed | CLI unit test and `crates/cli/tests/commands.rs` |
| Real HTTP/control/transport and listener startup tests | 12 passed | `crates/server/tests/http.rs` and listener startup unit test |
| HTTPS browser transport | 6 passed | `test-results/browser-report.json`, per-test journals/screenshots/traces |
| All 13 catalog cases | Exercised, reference unverified | generated `test-results/coverage.json` |
| Workspace Clippy | Passed with `-D warnings` | `.runtime/clippy-all.log`; one upstream `block` future-compatibility notice is not a project lint |
| Native completed-payment amount | 1 passed | `cargo test --locked -p paylink-gui`; completed browser amount is preserved until a new entry |
| Native macOS build | Passed | `cargo build --locked -p paylink-gui` |
| Native manual approved payment | Passed | keypad 26.00 → card → OK → Authorizing → Approved, observed through native UI |
| Native manual bank rejection | Passed | amount stayed 1.00 after attempted edit while active; card → OK → manual bank decline → Declined |
| Native selected terminal error | Passed | `terminal_unknown_0` → card → OK → Failed; exact catalog text visible on display |
| Native accessibility/shutdown | Passed | terminal status exposed in AX tree; last-window close terminated process |
| Docker without external network | Passed | built image; `--network none`; CLI arm/purchase/advance/assert reported one approval |
| Reference differential comparison | **Blocked** | no real PayLink/terminal recordings; runner exits 2 and writes `test-results/differential.json` |

The first implementation commit also passed all seven jobs in [CI run 35564299708](https://github.com/valtronforever/paylink-emulator/actions/runs/35564299708): headless tests and native builds on Linux, Windows and macOS, plus Linux browser tests. Later changes must pass the PR's latest run; see [PR #8 checks](https://github.com/valtronforever/paylink-emulator/pull/8/checks). Native runtime behavior has only been observed on macOS, not Windows/Linux desktops.

## Requirement audit

| Requirement | Implementation / status |
| --- | --- |
| Separate repository, Rust, CLI and optional GPUI | Implemented; headless default workspace excludes GUI |
| Pin a specific PayLink version | 2.1.20 win-x86 manifest, official URL and verified downloaded installer hash |
| Shared model controlled by API | Core → server; CLI and GUI use the same control endpoints |
| Physical-style terminal interaction | Generic original terminal skin, display, keypad, card, OK/Cancel; standalone vs armed browser flow |
| Every documented common error | 13-entry catalog, 11 terminal errors + service-down + setup-only; synthetic consequences explicitly marked |
| Manual actions and real delays | Manual/automatic stages, manual bank decision, per-phase timings, monotonic realtime and controlled clock |
| Real transport faults | Listener off, disconnect before acceptance/after acceptance/after outcome, truncated body, hang, malformed content and synthetic HTTP/body faults |
| Browser behavior | Actual HTTPS page → HTTP loopback listener, explicit CORS allowlist, no interception, recovery/reload/duplicate-charge tests |
| Isolation and reset | Fresh process per browser test; generation IDs; reset closes active and incomplete old sockets |
| Payment accounting evidence | Separate requests/accepted/approvals/reversals/socket-deliveries; correlated journal with logical/calendar time |
| Versioned control interface | `/control/v1`, JSON Schema generated from Rust, OpenAPI 3.1, token and command ID deduplication |
| Cross-platform and CI | Three-OS native build/headless test matrix, browser artifacts, container image/smoke |
| Reproducibility | Pinned compiler, GPUI family, Cargo/npm lockfiles, deterministic core clock/scenario/result seed |
| Real API fidelity and bank timing | **Blocked on reference captures**; unknown wire fields/codes and timing remain unverified |
| Reference fixtures / full API error list | **Blocked**; common wiki cases are not claimed to cover every API code |
| Inerix Devices settings and actual checkout UI | Separate integration [inerix#476](https://github.com/valtronforever/inerix/issues/476); not implemented in this emulator repository |

The emulator is an operational experimental test tool. This report does not certify PayLink/bank compatibility, physical EMV behavior or actual Inerix integration. No bank/protocol is listed as verified without reference evidence.
