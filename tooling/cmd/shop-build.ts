import { existsSync } from "fs";
import { join } from "path";
import { run } from "../lib/proc";
import { repoRoot } from "../lib/manifest";
import { resolveSecret } from "../lib/secrets";
import { TEST_TURNSTILE_KEY } from "../lib/docker";
import type { SiteManifest } from "../../sites";

/**
 * Build the site's storefront static export with values from the manifest +
 * decrypted secret, then stage it into ./shop-assets for the image COPY. The
 * Next export inlines SHOP_URL/SOCIAL_URL/PRIVACY_URL/TURNSTILE_SITE_KEY at
 * build time, so they are exported into the build env here. A stray .env.local
 * in the shop repo would shadow these, and the Turnstile key must not be the
 * test key, so both are guarded.
 */
export async function shopBuild(m: SiteManifest): Promise<void> {
  if (!m.shop) return;
  const root = repoRoot();
  const src = join(root, m.shop.src);
  const dist = join(root, m.shop.dist);

  if (!existsSync(src)) throw new Error(`shop source not found: ${src}`);
  const envLocal = join(src, ".env.local");
  if (existsSync(envLocal)) {
    throw new Error(`refusing to build: ${envLocal} would shadow production env; remove it first`);
  }

  const turnstile = await resolveSecret(m, m.shop.turnstileSiteKeyRef);
  if (!turnstile) {
    throw new Error(
      `Turnstile site key "${m.shop.turnstileSiteKeyRef}" not found ` +
        `(env ${m.shop.turnstileSiteKeyRef.toUpperCase()} or ${m.secrets.dir}/${m.shop.turnstileSiteKeyRef})`,
    );
  }
  if (turnstile === TEST_TURNSTILE_KEY) {
    throw new Error(`Turnstile site key for ${m.name} is Cloudflare's test key; set the real key`);
  }

  const env = {
    SHOP_URL: m.shop.shopUrl,
    SOCIAL_URL: m.shop.socialUrl,
    PRIVACY_URL: m.shop.privacyUrl ?? "",
    TURNSTILE_SITE_KEY: turnstile,
    SITE_DOMAIN: m.siteDomain,
  };

  await run(["bun", "install"], { cwd: src, env });
  await run(["bun", "run", "build"], { cwd: src, env });
  if (!existsSync(dist)) throw new Error(`shop build output not found: ${dist}`);

  // Stage into shop-assets/, keeping .gitkeep (mirrors the previous Makefile staging).
  await run(["sh", "-c", "find shop-assets -mindepth 1 ! -name .gitkeep -delete"], { cwd: root });
  await run(["sh", "-c", `cp -r "${dist}/." shop-assets/`], { cwd: root });
}
