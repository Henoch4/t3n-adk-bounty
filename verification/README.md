# WASM WIT verification — `wasm-tools component wit`

Run against the three built components with `wasm-tools 1.255.0`
(x86_64 windows, binary downloaded from bytecodealliance/wasm-tools releases —
`cargo install` was skipped after a disk-full; the prebuilt CLI is equivalent).

Command:
```
wasm-tools component wit <component>.wasm
```

## Result: all three components expose exactly the declared WIT world

| Component | Imports | Exports |
|---|---|---|
| `z_agent_paywall.wasm` (contract_id 559, v0.2.0) | `host:tenant/tenant-context@1.0.0`, `host:interfaces/logging@2.1.0`, `host:interfaces/kv-store@2.1.0` | `z:agent-paywall/contracts@0.2.0` — `check-gate`, `enter-gateway`, `pay-for-service` |
| `z_quota_counter.wasm` (contract_id 560, v0.3.0) | same host imports + `cluster-timestamp-secs` in tenant-context | `z:quota-counter/contracts@0.3.0` — `check`, `consume`, `reset` |
| `z_tenant_flight.wasm` (reference, id 539) | adds `host:interfaces/http@2.1.0` + `http-with-placeholders@2.1.0` | `z:tenant-flight/contracts@0.4.0` — `search-offers`, `book-offer` |

Note: the reference flight contract depends on `http` + `http-with-placeholders`
for Duffel search/book; the custom paywall/quota contracts import only what they
use (the unused `http` imports are pruned at link time). The quota counter now
strictly imports the host interfaces it calls, matching the WIT world in `wit/`.

Full outputs committed alongside this file:
- `z_agent_paywall.wit.txt`
- `z_quota_counter.wit.txt`
- `z_tenant_flight.wit.txt`

This is the walkthrough's verification step; registration acceptance on the
cluster is corroborated by this binary-level WIT extraction.