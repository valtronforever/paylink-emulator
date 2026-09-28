# Windows: real PayLink + simulated SSI JSON

This is the issue #9 research bench. It does not certify a bank, physical terminal,
EMV, USB driver, or Inerix integration. The original documentation acceptance gate
is unchanged. See [observations and remaining work](SSI_FINDINGS.md).

The responder uses Node's built-in TCP/HTTP modules, independently of the Rust
emulator model and response mapper. Node is already required by the differential
runner; there are no additional npm runtime dependencies. GPUI is not needed.

## Versions and prerequisites

- Windows x64 with .NET Framework 4.8 (the target application is win-x86).
- Node 24, `npm ci` using the lockfile, Git, PowerShell.
- Rust **1.98.1**, MSVC Build Tools and Windows SDK for building the emulator.
- PayLink installer SHA-256:
  `62d0e7a539380937ecb5af2f1c50438e4b84a070de4a20c1c848c11a5e395491`.
- POSServer assembly **2.1.20.10**, executable SHA-256:
  `85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465`.
- SSI document **1.4.6, 2026-09-12**, SHA-256:
  `8e4b459ade28f4ee301de7751659d4bf365f9c0f2ac3508a6745a597158e1a65`.
  This document postdates the target application. Wire version is byte **01**.

Download the installer from the profile manifest and the PDF from the URL in
`profiles/desktop-paylink-2.1.20-win-x86/ssi/inventory.json`. Keep binaries, the PDF,
certificates and keys in ignored `.runtime/ssi-lab/`; do not commit them. A changed
hash is a different experiment, not permission to update a pin silently.

Install PayLink into an isolated directory. The observed installation directory
was `.runtime/ssi-lab/paylink`; this is a chosen directory, not a vendor default.
Start its `POSServer.exe` from that directory, with stdout/stderr redirected to
local files. It runs headlessly. Use `Start-Process -WindowStyle Hidden` for this
background server. The GUI/updater need not run. Inspect `config/server.json` and
disable `updateOnStartApplication` for this isolated installation; record all
other update settings. Recheck the executable hash after every run.

The observed server reported HTTP `http://127.0.0.1:9020` and a separate native TCP
listener on 9021. It actually bound HTTP to `[::]:9020`; do not assume the config's
`server=127.0.0.1` restricts its listening interface. Use an isolated test host.
No PayLink HTTPS listener was observed. Always read the current server log rather
than assuming these ports on another installation.

## Start the bench

Before starting components, check the four reserved ports and pinned binaries:

```powershell
./scripts/lab/preflight.ps1 `
  -PayLinkExe .runtime/ssi-lab/paylink/POSServer.exe `
  -Installer .runtime/ssi-lab/installer.exe `
  -SsiSpecification .runtime/ssi-lab/spec.pdf
npm ci
npm run test:lab
cargo build --locked -p paylink-emulator
$env:SSI_CONTROL_TOKEN = [guid]::NewGuid().ToString('N')
$env:PAYLINK_CONTROL_TOKEN = [guid]::NewGuid().ToString('N')
```

Use separate PowerShell sessions, with the same respective tokens in the runner
session. Keep tokens out of config files, Git and browser pages.

```powershell
node scripts/ssi-terminal.mjs serve --port 3000 --control-port 13001 `
  --wire .runtime/ssi-lab/ssi-wire.jsonl
```

```powershell
./target/debug/paylink-emulator.exe serve `
  --payment-addr 127.0.0.1:13010 --control-addr 127.0.0.1:13011 `
  --allow-origin http://127.0.0.1:13020 `
  --journal .runtime/ssi-lab/emulator-journal.json
```

Both commands print JSON readiness. Save it; use actual returned addresses.
The emulator's default 3000/3001 conflicts with SSI; never use those defaults here.

If MSVC is unavailable, an alternative for *baseline* research is the Windows CLI
artifact from CI run `35569262537`, commit
`32ea20432597141d9fab3effa646353a1405ffe6`. Record its provenance and hash; it does
not contain this branch's fixes. Its observed executable SHA-256 was
`8e49c164459079279e053cde7f77060142ba732814cf33d092d224887acbc09d`.

## Configure the real PayLink

Export `/swagger/v1/swagger.json` and inspect `/api/pos/setup_map` from the running
build. This build offers `Verifone_All` for bank `none` and protocol `SSIJson`;
`Verifone X990` is terminal information returned by the simulator, not a value in
the OpenAPI model enum. Do not guess enums or use fiscal Checkbox OpenAPI.

The checked-in `examples/ssi/lab.json` contains the exact synthetic device DTO.
Register it using the **real** documented `/api/pos/devices/register` endpoint:

```powershell
$lab = Get-Content examples/ssi/lab.json -Raw | ConvertFrom-Json
Invoke-RestMethod "$($lab.reference_url)/api/pos/devices/register" `
  -Method Post -ContentType application/json `
  -Body ($lab.device | ConvertTo-Json -Depth 10)
```

This creates a test device on this isolated PayLink only. The settings are:
`mode=tcp`, `host=127.0.0.1`, `port=3000`, `merchant_id=TEST-MERCHANT`,
`resolve_ip_by_mac=false`. Loopback was actually exercised. No discovery scan or
bank connection is used. Registration is in-memory until PayLink's documented
saveconfig command is used; re-register after a fresh server start when needed.

Run preflight again with `-Running`. It checks the actual POSServer process path,
assembly/hash, Windows version and listener ownership. Save the result with the
run. Logs were discovered via startup output at `<installation>/logs/pos_YYYYMMDD.log`;
update `native_logs` in the local config to the paths actually observed that day.

## Scenario and control contract

Only the responder uses `GET /health`, `/state`, `/events?after=N`, and
`POST /arm`, `/reset`. These are **not PayLink routes**. Every request needs the
SSI bearer token. Browser Origin requests are rejected. Both listeners bind
127.0.0.1. Body limit is 64 KiB; the in-memory event window is 10,000 entries with
explicit truncation metadata; the append-only file retains the complete trace.

CLI equivalents:

```powershell
node scripts/ssi-terminal.mjs arm --file my-scenario.json
node scripts/ssi-terminal.mjs state
node scripts/ssi-terminal.mjs events
node scripts/ssi-terminal.mjs reset
```

Example scenario:

```json
{"id":"slow-approval","phases":[{"status":"S02","ms":1500},{"status":"S04","ms":2500}],"error_code":"","response_code":"0000","transaction_result":"APPROVED-ONLINE","faults":{"GetLastResult":"close"}}
```

`phases` are controlled test delays, not bank speed measurements. A Purchase
snapshots its scenario. Later arm calls apply to the next operation. Completion
is independent of response delivery and client connection. `GetLastResult` reads
the completed result. `GetResultByUid` supports the current/last operation only;
historical UID lookup and deduplication are not implemented. `Interrupt` only
cancels S02/S03/S08. Unknown methods return E05 and are logged.

`error_code` injects a terminal operation error; `request_errors` maps specific
methods to explicit errorCode replies. Do not equate request errors with financial
declines. The harness permits deliberate invalid method/code combinations; those
must be labelled invalid-message experiments, not normative protocol coverage.

Per-method `faults`: `silence`, `close`, `bad_lrc`, `bad_json`, `bad_length`,
`unknown_status`, `normal`. `response_delay_ms` delays delivery independently of
completion. `fragment_bytes`/`fragment_delay_ms` split response frames.
`result_fields` allows explicit synthetic terminalId, pan, cardHolderName and
bankName, so identical test data can be used across implementations.

Reset destroys open sockets, invalidates pending deliveries and clears active and
last operations. `{"preserve_result":true}` preserves the completed result while
resetting everything else. A process restart loses memory; it does **not** model
the real terminal's durable result. Restart persistence is still unsupported.
Inbound incomplete frames expire after 20 seconds; intentional response silence
does not close the socket. Invalid prefixes close the connection without trying
to find a payment command inside corrupted bytes.

## Recording, replay and browser evidence

Copy the example lab config locally and set actual executable/PDF/log paths and
URLs. `node scripts/record-paylink.mjs --config <local-config.json>` creates a new
`runs/<run-id>` with manifest, scenarios, HTTP records, raw SSI, native logs,
emulator journals and comparison report. Existing output directories are refused.
Control tokens are read only from environment variables. A client timeout stops
the suite: it is not cancellation, so inspect the native operation and recover
before resetting. The silence/recovery scenario can take about 3 minutes.

The recorder checks pinned hashes before and after and exports the real local
OpenAPI. Raw HTTP retains headers, bytes, non-JSON errors and durations. Native DB
backup is not automatic; use SQLite's backup API for a consistent snapshot and
retain it locally. Missing layers are explicitly listed in the manifest.

`node scripts/lab/make-matrix.mjs <config> <output>` generates all E00–E22 injection
cases. This probes the parser/mapper; it does not demonstrate that every error is
naturally reachable from Purchase. Inspect each permitted phase and method.

After inspecting synthetic data, export explicitly:

```powershell
node scripts/lab/export-fixtures.mjs runs/<id> --reviewed-synthetic
$env:PAYLINK_PAYMENT_URL = 'http://127.0.0.1:13010'
$env:PAYLINK_CONTROL_URL = 'http://127.0.0.1:13011'
node scripts/differential.mjs --require-reference
```

Export screens sensitive key names but is not a general redaction guarantee.
Native logs/DBs/certificates are excluded. Reference fixtures link SHA-256-checked
raw evidence. Explicit ID mappings preserve original bytes. Only listed volatile
IDs/timestamps are normalized; operation/transactionUid linkage is checked first.
`partial` and exit 2 mean unsupported recordings remain, not complete compatibility.

For browser evidence: `npx playwright install chromium`, then
`node scripts/lab/browser.mjs <config> runs/<browser-id>`. An installed Edge can be
selected with `"browser_channel":"msedge"`. The browser receives only payment URLs.
No route mocking or security-disabling flags are used. For HTTPS, supply
`browser_tls.key` and `.cert` whose certificate the host trusts and allow the exact
HTTPS origin on the emulator. Plain HTTP evidence does not verify HTTPS/LNA.

## Troubleshooting

- `link.exe not found`: install MSVC C++ tools and SDK, or use a pinned CI binary
  for baseline research. Rust installation alone is insufficient.
- Bind error: inspect listener ownership; do not kill unrelated applications.
- HTTP 403 from control: token mismatch or an Origin header.
- No SSI frames: inspect the real registered device/protocol/address, not just HTTP
  readiness. Ping in this build uses GetTerminalInfo.
- HTTP request timed out: leave SSI state intact until PayLink finishes recovery;
  record later logs and the result separately.
- Unknown SSI method: preserve trace and add only the required documented method.
- Chromium installation incomplete: use installed Edge via Playwright, recording
  its version; never call that a Chromium-version-equivalent run.
