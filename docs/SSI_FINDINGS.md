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
baseline emulator in Edge 154. HTTPS→localhost, browser LNA permissions, full
concurrency/cancellation/restart matrices and repeated timeout-boundary measurements
remain separate unverified items. Inerix product integration remains dependent on
Inerix #476. These gaps mean **issue #9 is not yet fully accepted**.

The profile's global `reference_compatibility` remains unverified and
`verified_banks` remains empty. Individual recordings prove only this build's
response to the supplied simulator bytes. The previous documentation-based
acceptance remains valid independently.
