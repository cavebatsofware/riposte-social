/**
 * Per-site manifest: the single source of truth for a deployment's NON-secret
 * identity. Build, deploy, provision, and dev tooling all read a site from here
 * instead of from scattered Makefile vars, sites/<name>.env, and
 * deploy/.env.prod.<site>. Secret VALUES never live here; only their names, so
 * the resolver (SOPS at deploy time) can look them up.
 */

export type AppRole = "social" | "shop" | "both";

export interface ImageRef {
  registry: string; // ghcr.io
  owner: string; // cavebatsofware
  repo: string; // riposte-social
  tag: string; // latest | cavebatsoftware
}

export interface ComposeConfig {
  service: string; // compose service name (app | app-cavebat)
  envFile: string; // non-secret runtime env, relative to repo root
  secretsDir: string; // where decrypted plaintext is materialized for compose secrets:
  socialPort: number; // host loopback port for the social app
  shopPort?: number; // host loopback port for the storefront, when appRole serves a shop
}

export interface DbConfig {
  host: string; // postgres
  port: number; // 5432
  role: string; // login role the app connects as
  database: string; // database the app uses
}

export interface ShopConfig {
  src: string; // storefront project dir (relative to repo root)
  dist: string; // its static-export output dir
  shopUrl: string; // public storefront origin, baked into the export
  socialUrl: string; // social app origin the storefront links to, baked into the export
  privacyUrl?: string;
  turnstileSiteKeyRef: string; // name of the secret holding the Turnstile site key
}

export interface SiteManifest {
  name: string; // registry key, image tag, secrets subdir identity
  siteDomain: string; // bare domain, read at runtime by the app as SITE_DOMAIN
  siteUrl: string; // canonical social app origin
  siteName: string; // display name (also seeded as the site_name setting)
  appRole: AppRole;
  image: ImageRef;
  compose: ComposeConfig;
  db: DbConfig;
  shop?: ShopConfig; // present iff appRole serves a shop
  secrets: { dir: string; names: string[] };
}

/** Full image reference, e.g. ghcr.io/cavebatsofware/riposte-social:cavebatsoftware. */
export function imageRef(m: SiteManifest): string {
  const { registry, owner, repo, tag } = m.image;
  return `${registry}/${owner}/${repo}:${tag}`;
}

// A bare hostname: labels of letters/digits/hyphens joined by dots, no scheme or path.
const BARE_DOMAIN = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/;

function hostOf(url: string): string | null {
  try {
    return new URL(url).host;
  } catch {
    return null;
  }
}

/**
 * Throw if a manifest would produce a broken or mis-identified build. These are
 * the guard rails for the incidents this tooling exists to prevent: an empty or
 * wrong baked domain, and a shop missing its Turnstile key reference.
 */
export function validateManifest(m: SiteManifest): void {
  const err = (msg: string): never => {
    throw new Error(`site "${m.name || "(unnamed)"}": ${msg}`);
  };

  if (!m.name) err("name is required");
  if (!m.siteDomain) err("siteDomain is empty (the app needs it at runtime as SITE_DOMAIN)");
  if (/[:/]/.test(m.siteDomain)) err(`siteDomain must be a bare domain, not a URL: ${m.siteDomain}`);
  if (!BARE_DOMAIN.test(m.siteDomain)) err(`siteDomain is not a valid domain: ${m.siteDomain}`);

  // siteUrl host must sit within siteDomain. This catches a dropped-letter domain
  // typo (e.g. cavebatsofware.com vs cavebatsoftware.com): social.cavebatsoftware.com
  // does not end with cavebatsofware.com, so the mismatch is rejected here.
  const host = hostOf(m.siteUrl);
  if (!host) err(`siteUrl is not a valid URL: ${m.siteUrl}`);
  if (host !== m.siteDomain && !host!.endsWith(`.${m.siteDomain}`)) {
    err(`siteUrl host (${host}) is not within siteDomain (${m.siteDomain}); likely a domain typo`);
  }

  const wantsShop = m.appRole === "shop" || m.appRole === "both";
  if (wantsShop && !m.shop) err(`appRole=${m.appRole} requires a shop config`);
  if (!wantsShop && m.shop) err(`appRole=${m.appRole} must not define a shop config`);

  if (m.shop) {
    if (!m.shop.turnstileSiteKeyRef) err("shop.turnstileSiteKeyRef is required");
    if (!hostOf(m.shop.shopUrl)) err(`shop.shopUrl is not a valid URL: ${m.shop.shopUrl}`);
    if (!hostOf(m.shop.socialUrl)) err(`shop.socialUrl is not a valid URL: ${m.shop.socialUrl}`);
  }

  for (const need of ["postgres_password", "secure_values_key"]) {
    if (!m.secrets.names.includes(need)) err(`secrets.names must include "${need}"`);
  }
  if (wantsShop && !m.secrets.names.includes(m.shop!.turnstileSiteKeyRef)) {
    err(`secrets.names must include the shop Turnstile key ref "${m.shop!.turnstileSiteKeyRef}"`);
  }
}
