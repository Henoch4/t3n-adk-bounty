import {
  T3nClient,
  setEnvironment,
  loadWasmComponent,
  getNodeUrl,
  eth_get_address,
  createEthAuthInput,
  createDefaultHandlers,
  metamask_sign,
  TenantClient,
} from "@terminal3/t3n-sdk";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const envFile = resolve(process.cwd(), "..", ".env");
for (const line of readFileSync(envFile, "utf8").split(/\r?\n/)) {
  const m = line.match(/^([A-Z0-9_]+)\s*=\s*(.*)$/);
  if (m && !(m[1] in process.env)) process.env[m[1]] = m[2];
}

setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY!;
const wasmComponent = await loadWasmComponent();
const nodeUrl = getNodeUrl();
const trustAnchor = { unsafe_trust_server: true } as const;

const address = eth_get_address(T3N_API_KEY);
const t3n = new T3nClient({
  wasmComponent,
  baseUrl: nodeUrl,
  trustAnchor,
  handlers: {
    ...createDefaultHandlers(nodeUrl, trustAnchor),
    EthSign: metamask_sign(address, undefined, T3N_API_KEY),
  },
});

await t3n.handshake();
const did = await t3n.authenticate(createEthAuthInput(address));
const tenantDid = did.value;
console.log("Connected as:", tenantDid);

const tenant = new TenantClient({ t3n, baseUrl: nodeUrl, tenantDid });
const me = await tenant.tenant.me();
console.log("TenantClient ready. me() quotas =", JSON.stringify(me));

const ROOT = resolve(process.cwd(), "..");

async function registerContract(
  tail: string,
  wasmRel: string,
  version: string
) {
  const wasmBytes = readFileSync(resolve(ROOT, wasmRel));
  const res = await tenant.contracts.register({ tail, version, wasm: wasmBytes });
  console.log(`[${tail}] registered:`, JSON.stringify(res));
  return res.contract_id as number;
}

async function ensureMap(tail: string, contractIds: number[]) {
  try {
    await tenant.maps.create({
      tail,
      visibility: "private",
      writers: { only: contractIds },
      readers: { only: contractIds },
    });
    console.log(`[map] ${tail} created`);
  } catch (e) {
    // Re-registration mints a fresh contract id — re-scope the ACL (BUGS.md #2).
    try {
      await tenant.maps.update(tail, {
        writers: { only: contractIds },
        readers: { only: contractIds },
      });
      console.log(`[map] ${tail} ACL -> ${contractIds.join(",")}`);
    } catch (e2) {
      console.log(`[map] ${tail} (create+update failed): ${(e as Error).message}`);
    }
  }
}

async function run(tail: string, version: string, functionName: string, input: unknown) {
  try {
    const out = await tenant.contracts.execute(tail, { version, functionName, input });
    console.log(`[${tail}.${functionName}] =>`, JSON.stringify(out));
    return out;
  } catch (e) {
    console.log(`[${tail}.${functionName}] ERROR: ${(e as Error).message}`);
    return null;
  }
}

// ---- Paywall (z-agent-paywall) ----
const PAYWALL_TAIL = "agent-paywall";
const PAYWALL_VERSION = "0.2.1";
const paywallId = await registerContract(
  PAYWALL_TAIL,
  "shared-target/wasm32-wasip2/release/z_agent_paywall.wasm",
  PAYWALL_VERSION
);
await ensureMap("gate", [paywallId]);
// seed session budget: 500 cents, 50 cent per-call cap
try {
  await tenant.maps.entrySet(
    "gate",
    "meta:budget",
    JSON.stringify({ session_budget_cents: 500, per_call_cap_cents: 50 })
  );
  console.log("[gate] budget seeded");
} catch (e) {
  console.log("[gate] budget seed failed (will use contract defaults):", (e as Error).message);
}

// per-call cap = 50 cents. A 25c call passes, a 70c call is denied.
await run(PAYWALL_TAIL, PAYWALL_VERSION, "check-gate", {
  caller: "did:key:z6Mk_demoCaller",
  label: "resume-rewrite",
  amount_cents: 25,
});
await run(PAYWALL_TAIL, PAYWALL_VERSION, "check-gate", {
  caller: "did:key:z6Mk_demoCaller",
  label: "video-render",
  amount_cents: 70,
});
await run(PAYWALL_TAIL, PAYWALL_VERSION, "enter-gateway", {
  caller: "did:key:z6Mk_demoCaller",
  label: "resume-rewrite",
  amount_cents: 25,
});
await run(PAYWALL_TAIL, PAYWALL_VERSION, "enter-gateway", {
  caller: "did:key:z6Mk_demoCaller",
  label: "resume-rewrite-second",
  amount_cents: 25,
});
await run(PAYWALL_TAIL, PAYWALL_VERSION, "check-gate", {
  caller: "did:key:z6Mk_demoCaller",
  label: "third-call-over-budget",
  amount_cents: 400,
});
await run(PAYWALL_TAIL, PAYWALL_VERSION, "pay-for-service", {
  caller: "did:key:z6Mk_demoCaller",
  amount_cents: 10,
  currency: "usd",
});
const paywallLogs = await tenant.contracts.logs(PAYWALL_TAIL, { limit: 20 });
console.log("[agent-paywall] logs:", JSON.stringify(paywallLogs));

// --- Quota counter (z-quota-counter) ---
const QUOTA_TAIL = "quota-counter";
const QUOTA_VERSION = "0.3.1";
const quotaId = await registerContract(
  QUOTA_TAIL,
  "shared-target/wasm32-wasip2/release/z_quota_counter.wasm",
  QUOTA_VERSION
);
await ensureMap("quotas", [quotaId]);

await run(QUOTA_TAIL, QUOTA_VERSION, "consume", { key: "did:key:z6Mk_app1", limit: 5, amount: 2 });
await run(QUOTA_TAIL, QUOTA_VERSION, "consume", { key: "did:key:z6Mk_app1", limit: 5, amount: 2 });
await run(QUOTA_TAIL, QUOTA_VERSION, "consume", { key: "did:key:z6Mk_app1", limit: 5, amount: 2 });
await run(QUOTA_TAIL, QUOTA_VERSION, "check", { key: "did:key:z6Mk_app1" });
await run(QUOTA_TAIL, QUOTA_VERSION, "consume", { key: "did:key:z6Mk_app2", limit: 3, amount: 1 });
await run(QUOTA_TAIL, QUOTA_VERSION, "reset", { key: "did:key:z6Mk_app1" });
await run(QUOTA_TAIL, QUOTA_VERSION, "check", { key: "did:key:z6Mk_app1" });
const quotaLogs = await tenant.contracts.logs(QUOTA_TAIL, { limit: 20 });
console.log("[quota-counter] logs:", JSON.stringify(quotaLogs));

console.log("DONE paywallId=%s quotaId=%s", paywallId, quotaId);