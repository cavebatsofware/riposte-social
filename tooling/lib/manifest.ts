import { resolve } from "path";
import { SITES, validateManifest, type SiteManifest } from "../../sites";

/** Repo root, derived from this file's location (tooling/lib -> repo root). */
export function repoRoot(): string {
  return resolve(import.meta.dir, "..", "..");
}

/** Load and validate a site by name, or throw with the known names. */
export function loadSite(name: string): SiteManifest {
  const m = SITES[name];
  if (!m) {
    throw new Error(`unknown site "${name}". Known sites: ${Object.keys(SITES).join(", ")}`);
  }
  validateManifest(m);
  return m;
}
