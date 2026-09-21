# Control API v1

Base path: `/control/v1`. Every endpoint requires `Authorization: Bearer <run-token>` (at least 16 characters). Requests with `Origin` are denied even with a token: this is a native runner API, never a browser production integration. JSON body limit: 64 KiB. Default control port: 3001.

GET resources:

| Resource | Result |
| --- | --- |
| `health` | readiness, profile, generation |
| `profile` | target version, installer hash, compatibility status |
| `errors` | common-error catalog and evidence status |
| `state`, `journal` | full deterministic engine snapshot |
| `devices`, `operations`, `scenarios` | map keyed by identifier |
| `queue` | queued scenario snapshots and remaining uses |
| `events?after=42&wait_ms=1000` | cursor-based journal events; wait capped at 30 seconds |
| `transport` | actual payment-listener availability or bind error |

POST envelopes:

```json
{"command_id":"unique-run-command-1","generation":1,"payload":{}}
```

A repeated ID with identical content returns the same result. Different content with the same ID returns 409. `generation` is optional but recommended for delayed test commands: stale generations return 409. Reset clears old command IDs, then caches its own result, so retrying the same reset does not reset twice. Use IDs unique within the run, including across resets. This idempotency applies only to control commands, never PayLink purchases.

| Resource | Payload |
| --- | --- |
| `scenarios` | full scenario; save by `id` |
| `arm` | full scenario, or `{"scenario_id":"saved-id"}` |
| `devices` | `{"id":"...","name":"Virtual POS","merchant":"TEST-MERCHANT","online":true,"setup_error":null}` |
| `purchase` | `{"device_id":"...","amount":100}`; standalone model operation, no wire response |
| `action` | `{"operation_id":"g1-op1","event":"card_presented"}` |
| `advance` | `{"ms":100}`; controlled-clock mode only, max one hour per command |
| `transport` | `{"online":false}`; asynchronously unbind/rebind payment port, poll GET transport |
| `reset` | `{}`; clear model, scenarios, counters; increment generation; restore payment listener |
| `assert` | optional exact `requests`, `accepted`, `approvals`, `reversals`, `delivered`; booleans `queue_empty`, `idle` |

Errors use `{"error":{"code":"command_rejected","message":"..."}}` with 409 for invalid model commands, 401 for authorization, 404 for unknown reads, 500 for journal write failure. Malformed JSON/schema is rejected by the HTTP layer. Assertions fail with 409; CLI returns a nonzero process exit code.

## Scenario fields

`id`, `device_id`, optional exact `amount`/`merchant`, `uses` (1–10000), `mode` (`automatic`/`manual`), `outcome` (`approved`/`declined`/`error`), `error_id`, `failure_stage`, `timing`, `delivery`, `require_confirmation`, `manual_bank`, `seed`. Unknown fields are rejected. Amounts are 1–999999999 minor units. A first queued scenario for that device must match; mismatches do not skip to a later scenario or consume it.

Timing fields are `connect_ms`, `card_ms`, `customer_ms`, `authorize_ms`, `confirm_ms`, `response_ms`, `timeout_ms`. Each is at most one hour; timeout must be positive. Stage defaults are 300/2000/500/1200/500 milliseconds; response delay defaults to zero and timeout to 120000. These are synthetic, configurable defaults.

Stages: `connecting → awaiting_card → awaiting_customer → authorizing → approved/declined`. Optional synthetic confirmation adds `awaiting_confirmation`. Error scenarios fail at the selected active `failure_stage` (default `authorizing`). Terminal stages also include `cancelled`, `failed`, `timed_out`, `reversed`. Timeout wins a tie with a scheduled success; no charge occurs on that boundary.

Manual events:

| Event | Allowed state |
| --- | --- |
| `card_presented` | awaiting_card |
| `customer_confirmed` | awaiting_customer |
| `bank_approved`, `bank_declined` | authorizing with manual_bank enabled |
| `terminal_confirmed` | awaiting_confirmation |
| `customer_cancelled` | connecting, awaiting_card, awaiting_customer |
| `device_disconnected` | any active stage |
| `reversed` | approved and synthetic confirmation enabled |

Invalid transitions return an error and leave the model unchanged. Reconfiguring a busy device is rejected. Different devices can process independently. Late commands referencing pre-reset operation IDs fail.

## Payment plane

Provisional routes: GET `/api/devices/`, GET `/api/devices/{id}`, GET `/api/pos/{id}/ping`, POST `/api/pos/{id}/purchase` with `Content-Type: application/json` and `{"amount":100}`. No extra control fields/headers are required. Only explicitly allowed browser origins get CORS headers; preflight is supported. HTTPS applications must satisfy their browser's local-network policy. The control token must not be sent to the payment plane.

The default device ID is `00000000-0000-4000-8000-000000000001`. Every accepted payment snapshots a scenario. A disconnected client does not cancel the model. Inspect approvals separately from response delivery. `delivered` records a successful server socket write, not a client application acknowledgement: TCP cannot prove that the browser processed the body.

The small HTTP/1.1 listener accepts bounded Content-Length JSON requests, closes each response connection and rejects chunked request bodies. It exists to produce actual TCP faults, including incomplete response bodies. The exact reference server's keep-alive/chunked/header behavior is not yet verified.

Machine-readable specification: [OpenAPI 3.1](control-openapi.json). Its model schemas are generated from Rust via `cargo run -q -p paylink-core --example export_schema > docs/model-schemas.json`, then `node scripts/generate-openapi.mjs`.

All delivery policies: `normal`, `disconnect_before_accept`, `disconnect_after_accept`, `disconnect_after_commit`, `partial_response`, `hang`, `malformed_json`, `http400`, `http500`, `wrong_content_type`, `missing_fields`, `unknown_code`. The last cases are explicitly synthetic. A fault in delivery does not roll back a bank outcome.

Journal events carry profile, device/scenario/operation correlation, monotonic time, calendar time derived from the run epoch, and charge state when an operation is known. Controlled clock advances also advance this synthetic calendar; they do not change the host clock.
