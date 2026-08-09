# T3N ADK Bounty — Submitter Findings (BUGS.md)

Verified against **testnet** cluster, `@terminal3/t3n-sdk@4.30.0`,
Node v24.18.0, Rust 1.97.1, `wasm32-wasip2`, Aug 2026.

## Reprintable evidence

Every finding below was reproduced by me; the failing call + error are quoted
inline. Contract IDs reference this tenant:
`did:t3n:5db3681df85b9a698777a5aa603329da86cdb5dc`.

---

### 1. `token.balance` / `token.get-usage` are broken on this cluster (blocked credit proof)

`quickstart.ts` reproduces two SDK failures when querying the token ledger after
`T3nClient.authenticate()`:

- `getBalance()` →
  `DOMException [InvalidCharacterError]: Invalid character` (thrown in `atob`).
- `getUsage()` → `RPC Error: invalid token.get-usage params: invalid type:
  expected struct GetUsageParams ...`

Both are *self-only* sealed-session RPCs (the SDK seals `get-usage` / balance with
the session key rather than plaintext over TLS). Every documented way to show a
credit balance fails on a freshly-claimed wallet:

```
[errors]:
DOMException [InvalidCharacterError]: Invalid character
    at new DOMException (...)
RPC Error: invalid token.get-usage params: invalid type: expected struct GetUsageParams
```

**Repro**: `npx tsx my-t3n-app/quickstart.ts` (auth succeeds; both balance
calls throw).

**Impact**: a tenant cannot programmatically prove their credit balance via the
SDK/CLI — a required "claim free tokens" step in the walkthrough.

**Workaround**: none in SDK. `T3nClient.execute` runs `action.execute` with a
sealed blob the node rejects for `get-usage`. The CLI (`t3n token balance`)
hits the same code path.

**Severity**: medium-high (docs promise balance UX; both helper paths are dead on
this cluster).

---

### 2. Contract re-registration requires a strictly-increasing semver and re-mints KV map ACL

`tenant.contracts.register({ tail, version, wasm })` **rejects a re-upload of the
same version**:

```
RPC Error: contract version invalid: version 0.1.1 is not higher than current version 0.1.1
```

A fix cycle for a contract therefore *must* bump `version` (0.1.0 → 0.1.1 → 0.2.0).
Additionally, **each registration hands the map ACL a NEW `contract_id`**:

```
first deploy: contract_id 553  → maps.gate.quotas readers/writers = [553]
re-run @0.2.0: contract_id 555 → kv denied: TenantContract(.../555) cannot read map z:...:quotas
```

So after any re-registration the tenant must re-run `maps.update(tail, {
readers, writers })` with the new id — the SDK does not do this, and the
walkthrough's "register once" flow silently breaks the map ACL on any upgrade.

**Impact**: no in-place upgrade path; every hotfix is a manual `version` bump +
map re-grant + fresh contract id, and the previous set of map grants is orphaned.

**Severity**: medium (affects any iteration / CI push over a register+map pair).

---

### 3. Single-tenant `maps` namespace: `writers: { only: [contract_id] }` is lost on upgrade

`maps.update(tail, { writers: { only: [newId] }, readers: { only: [newId] } })`
— the only documented way to scope a map to a contract. Because of #2, the
"only" set becomes wrong the moment a contract is re-registered. There is no
`only: [contract owner]` affordance to keep the tenant itself writable while
giving a single contract read access.

**Severity**: low (design gap, not a crash).

---

### 4. `fuel_per_minute` is burned by ~10 KV-heavy calls (rate-limit trap)

A single demo script performing ~10 KV-heavy contract invocations + a
re-registration burns the cluster's `fuel_per_minute_max: 500_000_000` AND
the tenant's 20K test-credit grant. The `fuel_per_minute` cap surfaces as an
`RPC Error: quota exceeded (fuel_per_minute)` mid-script (captured on the
`reset`/`check` tail of `demo-quota.ts`, see `verification/LIVE_OUTPUTS.md`),
and once the credit grant is gone the account also hard-fails read-only
`contracts.logs` calls with `InsufficientCredit (required=10000000000,
available=0)` — see finding #8.

**Impact**: a developer iterating over contracts (build → register → verify →
log) hits hard rate-limiting after a handful of calls, with a per-minute
cool-down. There is no per-call fee display in the SDK, so the only signal the
developer gets is the `quota exceeded (fuel_per_minute)` RPC error.

**Severity**: medium. See `verification/LIVE_OUTPUTS.md` for the exact call
sequence that trips it.

---

### 5. `tenant.tenant.me()` returns quotas+status whose docs don't match runtime

The SDK docs (README, `TenantMeResponse`) describe the shape as `{tenant, label,
status, quotas}` — but never document the `max_inline_bytes` / `max_cas_bytes` /
`cas_retention_days: null` / `log_max_entries` fields that determine real
contract budget. The walkthrough's "check your quotas" snippet talks about a
smaller shape. (No functional break, purely a docs-vs-runtime diff.)

**Severity**: low — good for the Candidate notes, not a block.

---

### 6. WASM size limit vs default `opt-level = "s"` docs

`max_wasm_bytes: 1048576` (1 MiB) per contract is easily exceeded when `lto =
true` links `wit-bindgen` into the component for the `http` + `kv` interfaces
together. Our `z_agent_paywall.wasm` and `z_quota_counter.wasm` prune the
unused `http` imports (so each lands at ~160 KB — vs the flight reference
which keeps `http` + `http-with-placeholders` and is over 200 KB). Worth
calling out that a single 1 MiB cap is shared across all interfaces and any
extra host interface pull needs a bump.

**Severity**: low (docs gap worth confirming the cap is per-contract, not
cluster-wide).

---

### 7. (Environment note) `wasm-tools` cannot be be built on a 100%-full disk **issue**

Fixed locally (disk freed by removing `cargo-install*` temp dirs); not an SDK
issue — included for transparency because `wasm-tools component wit` is the
walkthrough's verification step.

---

### 8. Credit math is unit-denominated (1 token = 1,000,000 units) and the SDK error is confusing

After burning the testnet credit grant on the heavy `deploy-contracts.ts` +
`demo-quota.ts` cycle, a follow-up `demo-quota.ts` run is rejected with an
`InsufficientCredit` RPC error whose numbers looked like a units bug until the
1 token = 1,000,000 units conversion was applied:

```
InsufficientCreditError: InsufficientCredit
  (account=5db3681df85b9a698777a5aa603329da86cdb5dc,
   required=10000000000, available=0)
  code: 'RPC_ERROR',
  rpcMethod: 'action.execute',
  httpStatus: 403,
  detail: 'InsufficientCredit (account=5db..., required=10000000000, available=0)',
  required: 10000000000n,
  available: 0n
```

The triggering call is `tenant.contracts.logs(tail, { limit: 20 })` — a
read-only logs fetch — charged **10,000,000,000 units** (= 10,000 tokens) while
the initial testnet grant is **20,000 tokens** (the `available: 0` surfaced
after that grant was spent). The confusion: the error is denominated in
sub-units (1e6/token) with no unit label, so a freshly-claimed wallet shows
`required=10000000000` against a token-denominated balance and reads as a
catastrophic mismatch. After a 40,000-token top-up the same script ran
end-to-end with no credit errors — the gate is unit conversion + poor error
presentation, not an un-shipable fee floor.

**Repro**: exhaust the credit grant (a full `deploy-contracts.ts` run + the
per-minute `fuel_per_minute` cool-down waits), then `npx tsx demo-quota.ts`.
First `execute`/`contracts.logs` throws the error above until a top-up lands.

**Impact**: a candidate who burns the grant can't tell from the error how much
credit a call needs or what unit it's in — the free-token UX path (#1) is
already broken, so the only recourse is a manual wallet top-up.

**Severity**: low-medium (denomination/UX gap, not a functional block —
verifiable by top-up). Screenshot in `screenshots/demo-quota credit error.png`.

---

## SDK features confirmed GOOD (no action needed)

- `T3nClient.authenticate` (Eth sign), `setEnvironment("testnet")`, node URL
  resolution, `createDefaultHandlers` + `EthSign`.
- `TenantClient` + `tenant.contracts.register`, `maps.create/update/entrySet`
  paths — used for both our custom contracts.
- `http-with-placeholders` + `http` host interfaces reachable from wasip2.
- `logging.info` appears in `tenant.contracts.logs`.