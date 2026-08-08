import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { execSync } from "node:child_process";

const env = resolve(process.cwd(), "..", ".env");
const pairs = {};
for (const line of readFileSync(env, "utf8").split(/\r?\n/)) {
  const m = line.match(/^([A-Z0-9_]+)\s*=\s*(.*)$/);
  if (m) pairs[m[1]] = m[2];
}
const [cmd, ...args] = process.argv.slice(2);
process.env.T3N_API_KEY = pairs.T3N_API_KEY;
process.env.T3N_ENV = pairs.T3N_ENV;
const cli = "node \"" + resolve("node_modules/@terminal3/t3n-sdk/dist/cli/index.js") + "\"";
const out = execSync(cli + " " + [cmd, ...args].join(" "), { encoding: "utf8", env: {...process.env} });
console.log(out);