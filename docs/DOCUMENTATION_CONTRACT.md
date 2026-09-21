# Documentation-based PayLink contract

The user confirmed that no physical stand is available and delivery must rely on documentation. This replaces the earlier mandatory reference-capture gate. `compatibility: documentation_based` describes the evidence basis; `reference_compatibility: unverified` separately records that no physical certification is claimed.

## Sources and version

- [Checkbox connection and release documentation](https://wiki.checkbox.ua/app/pc/portal_acquiring).
- [Checkbox common terminal errors](https://wiki.checkbox.ua/app/pc/desktop_paylink_errors): the 13 cases in the versioned catalog.
- The profile [manifest](../profiles/desktop-paylink-2.1.20-win-x86/manifest.json) pins Desktop PayLink **2.1.20 win-x86**, installer checksum and assembly **2.1.20.10**. API XML documentation and previously extracted contract metadata are retained as supplementary version evidence; no further reverse engineering or hardware access is a delivery prerequisite.

## Supported contract

The supported browser workflow is discovery → ping → purchase with `amount` and optional `merchant_id`. API, CLI and native UI drive a shared model. All 13 documented common cases are selectable: 11 terminal failure scenarios, service unavailable (an actual stopped listener), and driver setup failure (device unavailability). The coverage report must prove model/API/browser exercise for the applicable categories.

Manual mode waits for card/customer/bank actions. Automatic mode advances through configured phases. Both preserve timeouts, busy state, payment outcome independently of response delivery, and a correlated journal. Each browser worker owns a separate process, ports, token and scenario queue.

## Explicit assumptions

| Documentation gap | Emulator rule | How to test/change it |
| --- | --- | --- |
| Exact phase durations and bank response time | Synthetic defaults: connect 300 ms, card 2000 ms, customer 500 ms, authorization 1200 ms, confirmation 500 ms; response 0 ms; deadline 120000 ms | Scenario `timing`; realtime clock for UI tests, controlled clock for exact boundaries |
| Human reaction time | A manual phase waits for its action while the deadline continues | Card/OK/cancel and manual bank actions through API/CLI/GUI |
| Bank-specific code and response channel for a common error | Catalog message and provisional failure wrapper; unresolved numeric mappings are not presented as real PayLink codes | Select `error_id`; coverage records exercise separately from reference compatibility |
| Driver installation failure | Setup-only unavailable device, not a fabricated completed payment response with code 9011 | Device `setup_error` and reconnect/recovery checks |
| Undocumented response-loss behavior | Delivery fault and bank outcome are independent | Disconnect before/after acceptance/commit, partial reply, malformed JSON, hang and HTTP fault scenarios |
| Retry without transaction identity | Each request can start a new operation; no automatic deduplication | Lost-reply/reload/retry browser test checks separate approval counts |
| Signature/reversal protocol | Synthetic, opt-in control-model capability; disabled in the default profile | `require_confirmation` and explicit actions; not advertised as a bank handshake |
| Restart persistence | Fresh model; journal is exported evidence, not automatic replay | Reset/generation/process-isolation tests |

The adapter explicitly rejects unsupported purchase parameters with HTTP 501 before consuming a scenario. Transaction `id`/deduplication, signature continuation, additional purchase options, registration, refunds, reports and WebSocket are outside this supported payment contract. This is a visible emulator limitation, not claimed real PayLink behavior. The native 169-value status inventory includes progress/success and other operations; it is not a list of 169 implemented payment errors.

## Acceptance

Required: supported contract tests, every common catalog case exercised, real socket/browser fault tests, native manual flow evidence, three-platform build/test CI, documented CLI/API usage and explicit assumptions. Physical terminal fixtures, bank certification and measured hardware latency are **not required**.

Optional later comparison: `node scripts/differential.mjs`. With no recordings it reports `not_run` and zero verified cases, without failing documentation-based delivery. `--require-reference` opts into a strict reference gate. Neither mode converts absent evidence into a reference pass.

Changes to a documented rule must update the contract revision, relevant scenarios and tests. New bank/protocol behavior should use named profiles instead of silently changing this pinned profile.
