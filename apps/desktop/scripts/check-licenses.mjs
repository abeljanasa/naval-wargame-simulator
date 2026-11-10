// npm licence audit for the shipped front end (master prompt §12): the npm counterpart of
// `cargo deny`. Only production dependencies are checked, because only those reach the installer.
// The allowlist matches deny.toml. GPL/AGPL/LGPL, missing and unknown licences fail.
// Run: npm run licenses

import { execFileSync } from "node:child_process";
import process from "node:process";

const ALLOWED = new Set([
  "MIT",
  "MIT-0",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "Unicode-3.0",
  "CC0-1.0",
  "BSL-1.0",
  "0BSD",
  "MPL-2.0",
]);

/** True if the SPDX expression can be satisfied using allowed licences only. */
export function satisfiable(expr) {
  const tokens = expr.replace(/[()]/g, " $& ").split(/\s+/).filter(Boolean);
  let i = 0;
  // Grammar: or := and ("OR" and)* ; and := atom ("AND" atom)* ; atom := id | "(" or ")"
  const atom = () => {
    const t = tokens[i++];
    if (t === "(") {
      const v = or();
      i++; // ")"
      return v;
    }
    return ALLOWED.has(t);
  };
  const and = () => {
    let v = atom();
    while (tokens[i] === "AND") {
      i++;
      v = atom() && v;
    }
    return v;
  };
  const or = () => {
    let v = and();
    while (tokens[i] === "OR") {
      i++;
      v = and() || v;
    }
    return v;
  };
  return or();
}

function productionPackages() {
  const out = execFileSync("npm", ["ls", "--omit=dev", "--all", "--json", "--long"], {
    encoding: "utf8",
    shell: process.platform === "win32",
    maxBuffer: 64 * 1024 * 1024,
  });
  const seen = new Map();
  (function walk(node) {
    for (const [name, dep] of Object.entries(node.dependencies ?? {})) {
      const id = `${name}@${dep.version}`;
      if (!seen.has(id)) {
        seen.set(id, dep.license);
        walk(dep);
      }
    }
  })(JSON.parse(out));
  return seen;
}

const isMain = process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/g, "/"));
if (isMain) {
  const pkgs = productionPackages();
  const bad = [...pkgs].filter(([, lic]) => typeof lic !== "string" || !satisfiable(lic));
  for (const [id, lic] of bad) console.error(`REJECTED ${id}: ${JSON.stringify(lic ?? null)}`);
  console.log(`npm licences: ${pkgs.size} production packages, ${bad.length} rejected`);
  process.exit(bad.length ? 1 : 0);
}
