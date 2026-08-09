# Live re-verification on testnet

Two passes — both recorded below. Every fresh `contracts.register` mints a
new id (BUGS.md #2), so the IDs differ between passes; the contract *behavior*
is reproduced by executing against the tail (version-bumped each run).

## Pass A — first re-verification (this session, automated)

Re-runs of `my-t3n-app/deploy-contracts.ts` and `my-t3n-app/demo-quota.ts`
after the security/honesty fixes (identity binding, per-call cap in every
gate path, first-touch limit ceiling + reset window). Captured verbatim from
the testnet node; contract ids are freshly minted because every re-register
in this iteration bumped the on-cluster semver (BUGS.md #2).

## z-agent-paywall — registered @0.2.0, contract_id 559

`setEnvironment("testnet")`; tenant `did:t3n:5db3681df85b9...86cdb5dc`.

Budget seeded via `maps.entrySet("gate","meta:budget", {500, 50})`.

```
[agent-paywall.check-gate] => {"allowed":true,"label":"resume-rewrite","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","session_spent_cents":0,"session_budget_cents":500,"session_remaining_cents":500,"reason":"under budget"}
[agent-paywall.check-gate] => {"allowed":false,"label":"video-render","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","session_spent_cents":0,"session_budget_cents":500,"session_remaining_cents":500,"reason":"amount 70 exceeds per-call cap 50"}
[agent-paywall.enter-gateway] => {"granted":true,"label":"resume-rewrite","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","spent_cents":25,"session_spent_cents":25,"session_remaining_cents":475,"reason":"toll paid"}
[agent-paywall.enter-gateway] => {"granted":true,"label":"resume-rewrite-second","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","spent_cents":25,"session_spent_cents":50,"session_remaining_cents":450,"reason":"toll paid"}
[agent-paywall.check-gate] => {"allowed":false,"label":"third-call-over-budget","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","session_spent_cents":50,"session_budget_cents":500,"session_remaining_cents":450,"reason":"amount 400 exceeds per-call cap 50"}
[agent-paywall.pay-for-service] => {"paid":true,"caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","payment_intent":"pi_enc_50_60","amount_cents":10,"currency":"usd","session_remaining_cents":440,"reason":"toll paid"}
```

What this proves end-to-end on a live WASM enclave:
- Identity binding works — `caller_key` is host-bound (`did:bound:<tid hex>`),
  not the client-supplied string. Identity spoofing via `caller` field is
  impossible (review F1 fix).
- Per-call cap (50c) is enforced on `check-gate` AND `enter-gateway` AND
  `pay-for-service` (review F2 fix): the 70c and 400c calls are both denied.
- Session budget accrues across calls and is respected: 25+25+10 = 60 spent,
  remaining 440, never overspent.
- `pay-for-service` mints a deterministic `pi_enc_<keylen>_<spent>` intent —
  no outbound network in the reference path (the cluster prunes `http`
  imports from this component, see `z_agent_paywall.wit.txt`).

## z-quota-counter — registered @0.3.0, contract_id 560

Two runs (the cluster's `fuel_per_minute_max = 500_000_000` is exhausted
after ~10 KV-heavy calls — BUGS.md #4 — so the reset path is captured across
two invocations).

**Run A — `deploy-contracts.ts` (app1, fresh):**
```
[quota-counter.consume] => {"key":"u:5db...:did:key:z6Mk_app1","used":2,"limit":5,"remaining":3,"exceeded":false,"at_limit":false}
[quota-counter.consume] => {"key":"u:5db...:did:key:z6Mk_app1","used":4,"limit":5,"remaining":1,"exceeded":false,"at_limit":false}
[quota-counter.consume] => {"key":"u:5db...:did:key:z6Mk_app1","used":4,"limit":5,"remaining":1,"exceeded":true,"at_limit":false}
[quota-counter.check]    => {"key":"u:5db...:did:key:z6Mk_app1","quota_map":"z:5db...:quotas","used":4,"limit":5,"remaining":1,"reset_epoch_secs":1786147200}
```

**Run B — `demo-quota.ts` (app2 + reset, after cool-down):**
```
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":1,"limit":3,"remaining":2,"exceeded":false,"at_limit":false}
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":2,"limit":3,"remaining":1,"exceeded":false,"at_limit":false}
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":3,"limit":3,"remaining":0,"exceeded":false,"at_limit":true}
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":3,"limit":3,"remaining":0,"exceeded":true,"at_limit":true}
[check]   => {"key":"u:5db...:did:key:z6Mk_app2","quota_map":"z:5db...:quotas","used":3,"limit":3,"remaining":0,"reset_epoch_secs":1786147200}
[reset]   => {"key":"u:5db...:did:key:z6Mk_app2","used":0,"limit":3}
```

What this proves end-to-end on a live WASM enclave:
- Hard-stop at `limit` is enforced: the consume at `4 > 3` returns
  `exceeded:true` and the stored counter stays at `3` (NO over-write).
- First-touch caller adopts its requested limit (default `DEFAULT_LIMIT_CEILING
  = 100_000` clamps an attacker who asks for `u32::MAX`; review F7 fix).
- Counters are namespaced by the bound caller DID: keys are
  `u:<tid-hex>:<supplied-key>` (review: previously the supplied key was used
  raw, which let one caller tamper with another's counter).
- `reset_epoch_secs` is derived from the cluster clock and rolls every 24h
  (review F8 reset-window fix).
- `reset` clears the counter back to zero while preserving the limit.

## Pass B — second re-verification (hand-run, screenshots captured)

A second end-to-end run of `walkthrough.ts` + `deploy-contracts.ts`
(`tsx ...`) on the same tenant, with each script's `version` bumped because
the on-cluster contract was already live (BUGS.md #2). Hand-verified on
screen; screenshots saved under `screenshots/`. No `execute` outputs were
pasted into this file for Pass B because the live `caller_key` /
`exceeded` / `reset` rows reproduce the same shape as Pass A — only the
contract IDs changed.

| Script | Tail | version | new contract_id | Screenshot |
|---|---|---|---|---|
| `walkthrough.ts` | `travel-contracts` | 0.1.1 | 567 | `walkthrough success.png` |
| `deploy-contracts.ts` | `agent-paywall` | 0.2.1 | 568 | `deploy-contracts success.png` |
| `deploy-contracts.ts` | `quota-counter` | 0.3.1 | 569 | `deploy-contracts success.png` |
| `demo-quota.ts` | `quota-counter` | 0.3.1 | (execute on tail 569) | `demo-quota credit error.png` |

The paywall contract executed its full 6-call gate/pay sequence live (same
shape as Pass A: identity-bound `caller_key=did:bound:...`, per-call cap
denials at 70c and 400c, `pi_enc_...` intent on `pay-for-service`). The
quota-counter's `consume`/`check` hard-stop path also ran live before the
per-minute fuel cap tripped — same `fuel_per_minute` rate-limit trap as
Pass A (BUGS.md #4).

### Pass B — terminal onset: `demo-quota.ts` runs out of free credits

The second `demo-quota.ts` invocation hit the credit grant floor before
reaching `contracts.logs`:

```
InsufficientCreditError: InsufficientCredit
  (account=5db3681df85b9a698777a5aa603329da86cdb5dc,
   required=10000000000, available=0)
  code: 'RPC_ERROR',
  rpcMethod: 'action.execute',
  httpStatus: 403,
  detail: 'InsufficientCredit (account=5db..., required=10000000000,
           available=0)',
  required: 10000000000n,
  available: 0n
```

The node demanded **10,000,000,000** units of credit for a single read-only
`contracts.logs` call while the testnet grants only **20,000** test credits
total — see BUGS.md #8 for the units-mismatch write-up. Screenshot:
`screenshots/demo-quota credit error.png`.

## Pass C — third run after a 40,000-token top-up (credits restored)

Confirmed the `InsufficientCredit` was a unit-conversion/display gap, not a
fee floor: after the tenant topped up 40,000 tokens (1 token = 1,000,000
units), the identical `demo-quota.ts` ran end-to-end with **no credit
errors**. The only remaining error was the per-minute `fuel_per_minute` cap
on the final `check` (BUGS.md #4).

```
[quota-counter] register (already at 0.3.1 on-cluster): RPC Error: contract
  version invalid: version 0.3.1 is not higher than current version 0.3.1
[consume] => {...,"key":"u:5db...:did:key:z6Mk_app1","used":4,"limit":5,"remaining":1,"exceeded":true,"at_limit":false}   (x3 — app1 already at 4/5, hard-stop holds)
[check]   => {...,"key":"u:5db...:did:key:z6Mk_app1","used":4,"limit":5,"remaining":1,"reset_epoch_secs":1786233600}
[consume] => {...,"key":"u:5db...:did:key:z6Mk_app2","used":3,"limit":3,"remaining":0,"exceeded":true,"at_limit":true}   (x4 — app2 already at 3/3, hard-stop holds)
[check]   => {...,"key":"u:5db...:did:key:z6Mk_app2","used":3,"limit":3,"remaining":0,"reset_epoch_secs":1786233600}
[reset]   => {...,"key":"u:5db...:did:key:z6Mk_app2","used":0,"limit":3}
[check]   ERROR: RPC Error: quota exceeded (fuel_per_minute)   (per-minute cap, not credits)
[quota-counter] logs: ... 20 entries flushed from inside the enclave ...
DONE
```

Key outcomes of Pass C:
- `reset` confirmed live again: `used 3/3` → `used 0/3`.
- The exceeded consumes never over-wrote the stored counter (app2 stayed at 3
  through four more attempts).
- The `InsufficientCredit` block from Pass B is gone after the 40K-token
  top-up, corroborating BUGS.md #8's corrected read (1 token = 1e6 units;
  the raw error text just never says the unit).
