import {
  T3nClient,
  setEnvironment,
  loadWasmComponent,
  getNodeUrl,
  eth_get_address,
  createEthAuthInput,
  createDefaultHandlers,
  metamask_sign,
} from "@terminal3/t3n-sdk";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// --- Load repo-root .env into process.env ---
const envFile = resolve(process.cwd(), "..", ".env");
for (const line of readFileSync(envFile, "utf8").split(/\r?\n/)) {
  const m = line.match(/^([A-Z0-9_]+)\s*=\s*(.*)$/);
  if (m && !(m[1] in process.env)) process.env[m[1]] = m[2];
}

// Sandbox/testnet cluster
setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY!;
const wasmComponent = await loadWasmComponent();

// v4.30 SDK: a trust anchor is required. fetchTrustedManifest() is a no-go on
// the testnet cluster (GET /api/trust-manifest -> 405, POST -> 400 — the
// manifest infra is not provisioned there). Use the documented escape hatch
// for clusters that publish no attestation bundle.
const nodeUrl = getNodeUrl();
const trustAnchor = { unsafe_trust_server: true } as const;

const address = eth_get_address(T3N_API_KEY);

// Default handshake handlers (ML-KEM key fetch, randomness) + an EthSign
// handler driven by the api key.
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

// Proof of claimed free test tokens.
// NOTE: t3n.getUsage() throws "invalid token.get-usage params ... expected
// struct GetUsageParams" on this SDK build (v4.30.0) — the sealed-session RPC
// sends a raw string where a struct is expected. getBalance() is the same
// sealed path and works; documented as a bug finding.
const balance = await t3n.getBalance();
console.log("Free credits available:", balance.available);
console.log("DID (short):", did.value);