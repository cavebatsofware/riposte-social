import { run } from "../lib/proc";
import { repoRoot } from "../lib/manifest";
import { devEnv, featuresFlag } from "../lib/site-env";
import type { SiteManifest } from "../../sites";

/**
 * Local dev with hot reload: start the dev postgres, build both SPAs once, then
 * watch the SPAs and the Rust server in parallel. SITE_DOMAIN comes from the
 * manifest; DEV_MODE + the e2e_testing feature enable the localhost overrides
 * (socket-address IP extraction, non-Secure cookies). Needs cargo-watch.
 */
export async function dev(m: SiteManifest): Promise<void> {
  const root = repoRoot();
  const env = { ...devEnv(m), SITE_DOMAIN: m.siteDomain, DEV_MODE: "true" };

  await run(["docker", "compose", "up", "-d", "postgres"], { cwd: root, env });
  await run(["bun", "run", "build:admin"], { cwd: root, env: { ...env, NODE_ENV: "production" } });
  await run(["bun", "run", "build:social"], { cwd: root, env: { ...env, NODE_ENV: "production" } });

  const cargoRun = `run --release ${featuresFlag(m, ["e2e_testing"]).join(" ")}`.trim();
  await Promise.all([
    run(["bun", "run", "build:watch"], { cwd: root, env }),
    run(["bun", "run", "build:watch:social"], { cwd: root, env }),
    run(["cargo", "watch", "-x", cargoRun], { cwd: root, env }),
  ]);
}
