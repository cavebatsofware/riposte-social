import { existsSync, mkdirSync, chmodSync, rmSync } from "fs";
import { dirname, join, relative } from "path";
import { run, capture } from "./proc";
import { repoRoot } from "./manifest";
import type { SiteManifest } from "../../sites";

/**
 * Build-time-only secrets are consumed in memory (GHCR login, the shop Turnstile
 * key). The rest are materialized to plaintext files that compose mounts under
 * /run/secrets at runtime.
 */
const RUNTIME_SECRETS = new Set(["postgres_password", "secure_values_key", "aws_credentials"]);

/** Encrypted, local-only secrets file for a site (gitignored, backed up offline). */
export function encPath(m: SiteManifest): string {
  return join(repoRoot(), "deploy", "secrets-enc", `${m.name}.json`);
}

const cache = new Map<string, Record<string, string>>();

/** Decrypt a site's secrets file to a name->value map (cached per process). */
export async function loadEncrypted(m: SiteManifest): Promise<Record<string, string>> {
  const hit = cache.get(m.name);
  if (hit) return hit;
  const path = encPath(m);
  if (!existsSync(path)) {
    cache.set(m.name, {});
    return {};
  }
  const res = await capture(["sops", "-d", "--output-type", "json", path]);
  if (res.code !== 0) throw new Error(`sops decrypt failed for ${path}: ${res.stderr.trim()}`);
  const map = JSON.parse(res.stdout) as Record<string, string>;
  cache.set(m.name, map);
  return map;
}

/**
 * Encrypt a name->value map to the site's secrets file via sops. Runs from the
 * deploy dir (where .sops.yaml lives) so the secrets-enc/*.json creation rule
 * matches the relative path, and encrypts in place. The plaintext is removed on
 * any failure so it never lingers.
 */
async function encryptTo(path: string, map: Record<string, string>): Promise<void> {
  mkdirSync(dirname(path), { recursive: true });
  const deployDir = join(repoRoot(), "deploy");
  const rel = relative(deployDir, path);
  await Bun.write(path, JSON.stringify(map, null, 2) + "\n");
  const res = await capture(["sops", "--encrypt", "--in-place", rel], { cwd: deployDir });
  if (res.code !== 0) {
    rmSync(path, { force: true });
    throw new Error(`sops encrypt failed: ${res.stderr.trim()}`);
  }
}

/**
 * Create a site's secrets file with FRESHLY GENERATED values. For NEW sites only:
 * regenerating secure_values_key orphans encrypted data and a new postgres_password
 * breaks DB auth, so migrate an existing deployment with `secrets import` instead.
 * Never overwrites an existing file.
 */
export async function genSecrets(m: SiteManifest): Promise<void> {
  const path = encPath(m);
  if (existsSync(path)) {
    throw new Error(`secrets file already exists (never overwritten): ${path}. Edit with: secrets edit ${m.name}`);
  }
  const rand = async (bytes: number) => (await capture(["openssl", "rand", "-hex", String(bytes)])).stdout.trim();
  const initial: Record<string, string> = {};
  for (const name of m.secrets.names) {
    if (name === "postgres_password") initial[name] = await rand(24);
    else if (name === "secure_values_key") initial[name] = await rand(32);
    else initial[name] = ""; // aws_credentials, ghcr_token, turnstile_site_key: filled via edit
  }
  await encryptTo(path, initial);
  console.log(`created ${path}. Fill aws_credentials / ghcr_token / turnstile_site_key with: secrets edit ${m.name}`);
}

/**
 * Create a site's secrets file from EXISTING values, preserving them: reads each
 * secret from its current materialized plaintext file (secretsDir/<name>),
 * falling back to the matching env var. This is the safe migration path for a
 * live deployment. Never overwrites an existing encrypted file.
 */
export async function importSecrets(m: SiteManifest): Promise<void> {
  const path = encPath(m);
  if (existsSync(path)) {
    throw new Error(`secrets file already exists (never overwritten): ${path}`);
  }
  const dir = join(repoRoot(), m.secrets.dir);
  const map: Record<string, string> = {};
  const missing: string[] = [];
  for (const name of m.secrets.names) {
    const file = join(dir, name);
    if (existsSync(file)) map[name] = (await Bun.file(file).text()).replace(/\r?\n$/, "");
    else if (process.env[name.toUpperCase()]) map[name] = process.env[name.toUpperCase()]!;
    else {
      map[name] = "";
      missing.push(name);
    }
  }
  await encryptTo(path, map);
  console.log(`imported existing secrets into ${path}`);
  if (missing.length) console.log(`note: no value found for ${missing.join(", ")}; fill with: secrets edit ${m.name}`);
}

/** Open the site's secrets file decrypted in $EDITOR (sops re-encrypts on save). */
export async function editSecrets(m: SiteManifest): Promise<void> {
  const path = encPath(m);
  if (!existsSync(path)) throw new Error(`no secrets file yet: ${path}. Create with: secrets gen ${m.name}`);
  cache.delete(m.name);
  await run(["sops", path]);
}

/** Materialize runtime secrets into the compose-mounted plaintext files (0444, dir 0700). */
export async function materialize(m: SiteManifest): Promise<void> {
  const map = await loadEncrypted(m);
  const dir = join(repoRoot(), m.secrets.dir);
  mkdirSync(dir, { recursive: true });
  chmodSync(dir, 0o700);
  for (const name of m.secrets.names) {
    if (!RUNTIME_SECRETS.has(name)) continue;
    const value = map[name];
    if (value === undefined || value === "") {
      throw new Error(`secret "${name}" is missing/empty in ${encPath(m)}. Set it with: secrets edit ${m.name}`);
    }
    const file = join(dir, name);
    if (existsSync(file)) rmSync(file); // file is 0444; remove before rewriting
    await Bun.write(file, value);
    chmodSync(file, 0o444);
  }
}
