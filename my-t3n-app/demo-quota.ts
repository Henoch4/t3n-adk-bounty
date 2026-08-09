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
const tenant = new TenantClient({ t3n, baseUrl: nodeUrl, tenantDid });

const TAIL = "quota-counter";
const VERSION = "0.3.1";

// Each fresh register mints a new contract_id and re-scopes the map ACL
// (BUGS.md #2). 0.3.0 already on-cluster (id 560) — bump to 0.3.1 (id 569)
// lifts the strictly-higher gate. If 0.3.1 is already live too the register
// throws (caught below) and the run targets the tail-version execute works.
try {
  const wasmBytes = readFileSync(
    resolve(process.cwd(), "..", "shared-target", "wasm32-wasip2", "release", "z_quota_counter.wasm")
  );
  const reg = await tenant.contracts.register({ tail: TAIL, version: VERSION, wasm: wasmBytes });
  console.log("[quota-counter] re-registered:", JSON.stringify(reg));
  const newId = (reg as { contract_id: number }).contract_id;
  try {
    await tenant.maps.update("quotas", {
      writers: { only: [newId] },
      readers: { only: [newId] },
    });
    console.log("[quota-counter] quotas map ACL ->", newId);
  } catch (e) {
    console.log("[quota-counter] map ACL update:", (e as Error).message);
  }
} catch (e) {
  console.log("[quota-counter] register (already at " + VERSION + " on-cluster):", (e as Error).message);
}

async function run(functionName: string, input: unknown) {
  try {
    const out = await tenant.contracts.execute(TAIL, { version: VERSION, functionName, input });
    console.log(`[${functionName}] =>`, JSON.stringify(out));
    return out;
  } catch (e) {
    console.log(`[${functionName}] ERROR: ${(e as Error).message}`);
    return null;
  }
}

const app = { key: "did:key:z6Mk_app1", limit: 5 };
await run("consume", { ...app, amount: 2 });
await run("consume", { ...app, amount: 2 });
await run("consume", { ...app, amount: 2 }); // 6 > 5 => exceeded
await run("check", app);
await run("consume", { key: "did:key:z6Mk_app2", limit: 3, amount: 1 });
await run("consume", { key: "did:key:z6Mk_app2", limit: 3, amount: 1 });
await run("consume", { key: "did:key:z6Mk_app2", limit: 3, amount: 1 }); // hit limit 3
await run("consume", { key: "did:key:z6Mk_app2", limit: 3, amount: 1 }); // 4 > 3 => exceeded
await run("check", { key: "did:key:z6Mk_app2" });
await run("reset", { key: "did:key:z6Mk_app2" });
await run("check", { key: "did:key:z6Mk_app2" });
const logs = await tenant.contracts.logs(TAIL, { limit: 20 });
console.log("[quota-counter] logs:", JSON.stringify(logs));
console.log("DONE");