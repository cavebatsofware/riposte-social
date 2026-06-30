import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { repoRoot } from "./manifest";
import type { SiteManifest } from "../../sites";

/** Load a site's local dev env file (sites/<name>.env) if present. */
export function devEnv(m: SiteManifest): Record<string, string> {
  const out: Record<string, string> = {};
  const f = join(repoRoot(), "sites", `${m.name}.env`);
  if (!existsSync(f)) return out;
  for (const line of readFileSync(f, "utf8").split("\n")) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const i = t.indexOf("=");
    if (i === -1) continue;
    out[t.slice(0, i).trim()] = t.slice(i + 1).trim();
  }
  return out;
}

/**
 * cargo --features flag for a site: `business` when it serves a shop, plus any
 * extras (e.g. e2e_testing for dev/test). Returns [] when there are none.
 */
export function featuresFlag(m: SiteManifest, extra: string[] = []): string[] {
  const feats = [...extra];
  if (m.appRole !== "social") feats.push("business");
  return feats.length ? ["--features", feats.join(",")] : [];
}
