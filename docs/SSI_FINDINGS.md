# Recorded SSI bench findings

Target: POSServer 2.1.20.10 from the pinned Desktop PayLink 2.1.20 win-x86 installer.
Capture date: 2026-09-28. Source: `reference/evidence/` in the version profile.
Bank configuration: **none / SIMULATED**, not bank certification.

The real process ran on Windows against the Node SSI responder. No physical
terminal or banking service participated. The standalone emulator baseline came
from CI for commit `32ea20432597141d9fab3effa646353a1405ffe6`.

## Observed behavior

- Registration supports `none`, `Verifone_All`, `SSIJson`, TCP loopback port 3000.
- A successful ping issued `GetTerminalInfo`, not PingDevice. Its HTTP 200 body
  includes `message: "ping"`; the emulator omitted this field.
- A normal purchase issued Purchase, then GetStatus, then GetLastResult, using
  separate TCP connections. PayLink supplies transactionUid and returns the same
  value as its HTTP operation id. One short simulated operation took about 2.1 s.
  This is a sampled observation, not a universal polling/timeout constant.
- The success body includes aliases, totals, date/expiry, entry mode, provider and
  additional_properties. The prior emulator omitted these. The patch adds the
  observed shape with deterministic synthetic data and fixture regression tests.
  Its calendar derives from the run epoch in UTC; native local date_time is an
  explicitly variable field. Synthetic expiry follows the epoch month.
- E00–E22 all produced HTTP 503 for the recorded injections. Codes/text are retained
  per fixture; they are not interchangeable with the 13 wiki cases.
- E18/E19/E20/E22 fell back to native 9024; E21 mapped to 9307 (Need Z report).
  This may reflect an older SSI mapping in PayLink. Do not rewrite the spec or claim
  a physically valid financial decline from this observation alone.
- Setting `error=false`, responseCode `51`, transactionResult `DECLINED_ONLINE`
  still produced `success=true`. This deliberately inconsistent combination was
  captured locally and in the first browser harness; it is not a valid decline
  fixture. The declined fixture uses `error=true` / E21 and records the actual
  surprising 503/9307 response.
- Suppressing the Purchase reply leaves a completed simulator result. PayLink
  eventually attempts GetResultByUid. The first 125-second client deadline expired
  before PayLink's native 9016 response, demonstrating that client timeout and
  terminal completion/recovery are distinct. The native error occurred later.

## Coverage and limitations

`ssi/inventory.json` inventories 23 error codes, 9 status values and 31 financial
result examples. The document does not enumerate all acquirer response codes.
All error response injections have raw recordings. They are mapper probes:
E08-on-Purchase, for example, does not establish legitimate Interrupt reachability.
No blanket claim of all valid protocol error paths is made.

The SSI-specific error mapping is opt-in through `reference_error`. E00–E22 and
`transport_timeout` preserve the recorded status/body without replacing the generic
wiki error catalog. The timeout mapping supports an approved terminal operation
with a failed HTTP result. A plain generic declined scenario still differs from
the recorded E21 injection; select the explicit mapping for a comparable case.
Timing is reported as untested unless a fixture provides justified bounds.

Standalone browser HTTP/CORS recordings exist locally for real PayLink and the
calibrated emulator in Edge 154. The checked-in experiment package contains traces,
network/console events and screenshots for ping/approval/decline/timeout on both.
An HTTPS page with default permissions was denied loopback access before reaching
PayLink. Permission-granted HTTPS remains unverified; external-origin allowance
was blocked by the execution environment's automatic approval policy. No browser
security bypass was used. Inerix product integration remains dependent on #476.

## Repeated and extended measurements

The calibrated CLI at `a1da1d7` passed **27/27** recorded HTTP fixtures (zero skipped,
mismatched or errored). A paired sequence passed **5/5** comparisons: first purchase,
busy ping, busy purchase, a new purchase with amount 101 after release, and fragmented
ping. Native busy is HTTP **503**, code 9009; the previous HTTP 400 was corrected.
These checks compare payloads and status, not equal execution time.

Two full native silence/recovery runs returned 503/9016 at 166184.48 and 166198.58 ms
(14.10 ms spread). Purchase had already completed in the simulator. Explicit E20
replies to GetResultByUid prevent result recovery in this timeout scenario.

Four boundary captures bracket the native read deadline. With a 144000 ms delayed
Purchase reply, both requests completed through normal GetStatus/GetLastResult at
146147.39 and 146078.90 ms. With a 146000 ms delay, PayLink abandoned that reply and
successfully recovered through GetResultByUid at 145061.52 and 145055.20 ms. Observed
within-pair spreads are 68.48 and 6.32 ms. A one-second margin around each observed
pair is a local bench investigation threshold, not a general product SLA. Timing
compatibility of the Rust emulator remains unverified; its delays are configurable.

All S01–S07 held for three seconds reached HTTP success; S08 produced 503/9009.
S00 is present in ordinary completed-operation traces. All 31 financial-result
examples were injected with explicit errorCode/responseCode combinations. The
recordings establish only those combinations; SSI gives no complete normative
financial mapping. See each row in `ssi/inventory.json` for the exact input,
native reaction and limitations of emulator support.

The simulator's optional persistent last result survived a real process restart.
The HTTP last-result returned the same operation ID, but **no new SSI query** was
observed for that HTTP call: it demonstrates PayLink's retained result, not a fresh
terminal read. Simulator unit tests independently check restored UID lookup and
discarding active work. A native PayLink process restart was blocked by automatic
approval policy, so native active-operation/restart recovery remains unverified.

The exported native OpenAPI exposes `/cancel` as a financial Void operation, not
SSI Interrupt. No operation-interruption HTTP endpoint was established; phase
cancellation is covered only in the independent simulator model. Refund/void,
reports, historical UID reconciliation, deduplication and additional merchants
remain separate extensions to the current HTTP emulator contract.

Malformed-response/disconnect captures and post-fault ping results are included
under `reference/experiments/transport/`. They are distinct from explicit E-code
injections. Coalesced requests are covered by independent TCP tests; PayLink used
separate connections in the captured flows, so native coalescing is not claimed.

The native log path and console output were captured, and SQLite's backup API
produced an integrity-checked snapshot of `db/response.db`. Full logs/DB remain
local; the shareable package contains reviewed synthetic HTTP/SSI/browser records.
See `reference/experiments/index.json` for hashes and exact executable provenance.

The implementation and scoped evidence are ready for review. **Issue #9 remains
partially accepted** while native restart and permission-granted HTTPS are unverified;
the unsupported extensions above are not silently counted as passes.

The profile's global `reference_compatibility` remains unverified and
`verified_banks` remains empty. Individual recordings prove only this build's
response to the supplied simulator bytes. The previous documentation-based
acceptance remains valid independently.
