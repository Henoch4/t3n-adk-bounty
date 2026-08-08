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

// --- Load repo-root .env ---
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

// --- TenantClient ---
const tenant = new TenantClient({
  t3n,
  baseUrl: nodeUrl,
  tenantDid,
});
const me = await tenant.tenant.me();
console.log("TenantClient ready. me() =", JSON.stringify(me));

// --- Step 3: register the reference flight contract ---
const WASM_PATH = resolve(process.cwd(), "..", "z-tenant-flight", "target", "wasm32-wasip2", "release", "z_tenant_flight.wasm");
const wasmBytes = readFileSync(WASM_PATH);

const CONTRACT_TAIL = "travel-contracts";
const CONTRACT_VERSION = "0.1.0";
const registerResult = await tenant.contracts.register({
  tail: CONTRACT_TAIL,
  version: CONTRACT_VERSION,
  wasm: wasmBytes,
});
console.log("Registered contract:", JSON.stringify(registerResult));
const contractId = registerResult.contract_id;

// --- Step 3b: create the secrets map + grant ACL to the contract ---
try {
  await tenant.maps.create({
    tail: "secrets",
    visibility: "private",
    writers: { only: [contractId] },
    readers: { only: [contractId] },
  });
  console.log("secrets map created");
} catch (e) {
  console.log("secrets map (may already exist):", String((e as Error).message));
}

console.log("tenantDid:", tenantDid);
console.log("contractId:", contractId);