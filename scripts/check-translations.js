#!/usr/bin/env node
/*
 * Catalog completeness check for Riposte Social i18n.
 *
 * Walks `social-frontend/public/locales/en/*.json` (the source-of-truth
 * catalog set) and asserts every other locale defined in
 * `social-frontend/src/i18n.js::SUPPORTED_LOCALES` has the same keys —
 * no missing translations, no leftover keys from removed strings, no
 * accidental nesting drift.
 *
 * Exits 0 when all catalogs match en, 1 otherwise. Designed to run as a
 * lightweight CI gate; no test runner needed. Plug it into package.json
 * via `npm run check:i18n` and CI calls it directly.
 */
const fs = require("node:fs");
const path = require("node:path");

const LOCALES = ["en", "es", "fr", "zh", "de"];
const ROOT = path.join(__dirname, "..", "social-frontend", "public", "locales");
const REFERENCE = "en";

/** Recursively flatten a JSON object into dot-paths, e.g. `{a:{b:1}}` → `["a.b"]`. */
function collectKeys(obj, prefix = "") {
  const out = [];
  if (obj === null || typeof obj !== "object") return out;
  for (const k of Object.keys(obj)) {
    const path = prefix ? `${prefix}.${k}` : k;
    const v = obj[k];
    if (v !== null && typeof v === "object" && !Array.isArray(v)) {
      out.push(...collectKeys(v, path));
    } else {
      out.push(path);
    }
  }
  return out;
}

function loadCatalog(lng, ns) {
  const file = path.join(ROOT, lng, `${ns}.json`);
  const raw = fs.readFileSync(file, "utf8");
  return JSON.parse(raw);
}

function main() {
  const enDir = path.join(ROOT, REFERENCE);
  if (!fs.existsSync(enDir)) {
    console.error(`Missing reference locale directory: ${enDir}`);
    process.exit(1);
  }
  const namespaces = fs
    .readdirSync(enDir)
    .filter((f) => f.endsWith(".json"))
    .map((f) => f.replace(/\.json$/, ""));

  let failed = false;
  for (const ns of namespaces) {
    const enKeys = new Set(collectKeys(loadCatalog(REFERENCE, ns)));
    for (const lng of LOCALES) {
      if (lng === REFERENCE) continue;
      let other;
      try {
        other = loadCatalog(lng, ns);
      } catch (e) {
        console.error(`[${lng}/${ns}] ✗ failed to read: ${e.message}`);
        failed = true;
        continue;
      }
      const otherKeys = new Set(collectKeys(other));
      const missing = [...enKeys].filter((k) => !otherKeys.has(k));
      const extra = [...otherKeys].filter((k) => !enKeys.has(k));
      if (missing.length === 0 && extra.length === 0) {
        console.log(`[${lng}/${ns}] ✓ ${otherKeys.size} keys`);
      } else {
        failed = true;
        if (missing.length > 0) {
          console.error(
            `[${lng}/${ns}] ✗ missing ${missing.length} key(s):`,
            missing.slice(0, 10).join(", "),
            missing.length > 10 ? `…(+${missing.length - 10} more)` : "",
          );
        }
        if (extra.length > 0) {
          console.error(
            `[${lng}/${ns}] ✗ extra ${extra.length} key(s):`,
            extra.slice(0, 10).join(", "),
            extra.length > 10 ? `…(+${extra.length - 10} more)` : "",
          );
        }
      }
    }
  }

  if (failed) {
    console.error("\nCatalog completeness check FAILED.");
    process.exit(1);
  }
  console.log("\nAll catalogs match the reference set.");
}

main();
