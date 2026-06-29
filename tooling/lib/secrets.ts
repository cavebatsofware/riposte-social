import { existsSync } from "fs";
import { join } from "path";
import { repoRoot } from "./manifest";
import { loadEncrypted } from "./sops";
import type { SiteManifest } from "../../sites";

/**
 * Resolve a secret VALUE by name for a site. Resolution order:
 *   1. environment variable <NAME_UPPERCASE> (e.g. GHCR_TOKEN, TURNSTILE_SITE_KEY)
 *   2. the SOPS-encrypted secrets file (secrets-enc/<site>.json)
 *   3. the materialized plaintext file <secretsDir>/<name> (legacy fallback)
 * A single trailing newline is stripped to match src/secret.rs.
 */
export async function resolveSecret(m: SiteManifest, name: string): Promise<string | undefined> {
  const fromEnv = process.env[name.toUpperCase()];
  if (fromEnv) return fromEnv;

  const map = await loadEncrypted(m);
  if (map[name]) return map[name];

  const file = join(repoRoot(), m.secrets.dir, name);
  if (existsSync(file)) return (await Bun.file(file).text()).replace(/\r?\n$/, "");

  return undefined;
}
