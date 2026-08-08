# T3N ADK Bounty — Paywall Agent Gate + Quota Counter on TEE Contracts

Claimed **Agent ID + test tokens** and deployed a first **Rust→WASM TEE
contract** on the T3N testnet. Two custom contracts, both registered and invoked
live on-cluster:

| Contract | Tail | contract_id | What it proves |
|---|---|---|---|
| `z-agent-paywall` | `z:<tid>:agent-paywall` | 552 | Paywall gate — per-session budget enforced inside the enclave before an agent tool dispatch |
| `z-quota-counter` | `z:<tid>:quota-counter` | 555 | KV-backed per-caller quota counters with hard stop + reset |

Both were built from scratch on this machine (Rust 1.97 → `wasm32-wasip2`,
`wit-bindgen 0.49`), mirroring the reference `z-tenant-flight` crate, and
registered via the official `@terminal3/t3n-sdk`.

## Repo layout

```
my-t3n-app/        TypeScript SDK harness (tsx scripts)
  quickstart.ts    auth + balance (balance path is broken — see BUGS.md)
  walkthrough.ts   reference flight contract registration (contract_id 539)
  deploy-contracts.ts paywall + quota deploy, maps, budget seed, invocations
  demo-quota.ts    quota-counter re-register + per-caller demo
z-agent-paywall/   custom Gateway contract (WIT world + Rust src)
z-quota-counter/   custom Quota counter contract
z-tenant-flight/   reference contract (registered as travel-contracts)
verification/      wasm-tools `component wit` extracts (binary-level WIT proof)
BUGS.md            SDK/deliverable findings found while building
```

## How to run (Windows / bash adaptions inline)

```bash
# 1) keys (already in .env, git-ignored)
copy your T3N_API_KEY into .env  (DID + Stripe test key optional)

# 2) SDK scripts
cd my-t3n-app && npm install && npx tsx quickstart.ts
npx tsx walkthrough.ts
npx tsx deploy-contracts.ts   # paywall + quota live demo, with kv logs

# 3) Rust
cd z-agent-paywall && cargo build --target wasm32-wasip2 --release
cd z-quota-counter && cargo build --target wasm32-wasip2 --release
```

## Live outputs (screenshots in `screenshots/`)

### Paywall — per-session budget
```
[check-gate] 25c  => allowed true,  session 0/500, remaining 500
[check-gate] 70c  => allowed false (exceeds per-call cap 50)
[enter-gateway]   => granted true,  spent 25, remaining 475
[enter-gateway]   => granted true,  spent 50, remaining 450
[enter-gateway]  => granted false (over budget)
[pay-for-service] => payment_intent pi_py_..., 10c spent
```

### Quota counter — per-caller hard-cap
```
consume(app1, limit=5, x3)  => used 2,4, exceed@6
check(app1)                 => used 4 / 5
consume(app2, limit=3, x4)  => 1,2,3, exceed@4
reset(app2)                 => used 0 / 3
```

Logs flushed from inside the enclave via `logging.info` read back through
`tenant.contracts.logs(tail)`.

## Key design facts

- Contracts read / write `z:<tid>:` KV maps (`gate`, `quotas`, `secrets`).
- Stripe key + session budget live in KV, never in WASM globals.
- `http-with-placeholders` is declared but the pay-flow mints a deterministic
  `pi_...` ref locally — a live Stripe `POST /v1/payment_intents` swap is the
  documented next step (see deliverables).

## Bounty deliverables

- **Agent ID claimed** — DID `did:t3n:5db3681df85b9a698777a5aa60331...b5dc`
- **Free token proof** — SDK paths broken on this cluster; see BUGS.md #1 for
  the repro + CLI fallback.
- **Deployed TEE contract** — both contracts above, live on testnet.
- **Bonus** — both paywall (agent-paywall) + quota counter.
- **Findings** — `BUGS.md`.
- **Google Doc** — link in the bounty submission sheet.
- **Screenshots** — `screenshots/`.

## License

MIT (contract crates mirror the `z-tenant-flight` repo license).