import { tmpdir } from "os";
import { join } from "path";
import { run, capture } from "./proc";
import { repoRoot } from "./manifest";
import { imageRef, type SiteManifest } from "../../sites";

/** Cloudflare's always-pass Turnstile test key; must never ship to production. */
export const TEST_TURNSTILE_KEY = "1x00000000000000000000AA";

/** Cargo features for the image: shop/both compile the `business` feature. */
function cargoFeatures(m: SiteManifest): string {
  return m.appRole === "social" ? "" : "business";
}

/**
 * Build the image and return its content id (sha256). The site domain is NOT
 * baked: the admin reads it at runtime from /api/auth/config, so the image is
 * site-agnostic. Captures the id via --iidfile and does NOT tag :latest, so a
 * later push targets exactly this artifact (no stale-:latest re-tag).
 */
export async function dockerBuild(m: SiteManifest): Promise<string> {
  const iidFile = join(tmpdir(), `rs-iid-${m.name}-${process.pid}`);
  const args = ["docker", "build"];
  const features = cargoFeatures(m);
  if (features) args.push("--build-arg", `CARGO_FEATURES=${features}`);
  args.push("--iidfile", iidFile, ".");
  await run(args, { cwd: repoRoot() });
  return (await Bun.file(iidFile).text()).trim();
}

async function grepInImage(iid: string, needle: string, paths: string[]): Promise<boolean> {
  const res = await capture([
    "docker", "run", "--rm", "--entrypoint", "sh", iid,
    "-c", `grep -Frlh -- '${needle}' ${paths.join(" ")} 2>/dev/null | head -n1`,
  ]);
  return res.stdout.trim().length > 0;
}

/**
 * Verify the just-built shop assets before any push. The site domain is runtime
 * now, so the only baked values to check are the shop's: it must not ship
 * Cloudflare's test Turnstile key, and must carry its own shop URL.
 */
export async function verifyBaked(iid: string, m: SiteManifest): Promise<void> {
  if (!m.shop) return;
  if (await grepInImage(iid, TEST_TURNSTILE_KEY, ["/app/shop-assets"])) {
    throw new Error(`verify failed: shop shipped the Turnstile TEST key (${TEST_TURNSTILE_KEY})`);
  }
  const shopHost = new URL(m.shop.shopUrl).host;
  if (!(await grepInImage(iid, shopHost, ["/app/shop-assets"]))) {
    throw new Error(`verify failed: shop URL host "${shopHost}" not found in shop-assets`);
  }
}

export async function dockerLogin(registry: string, user: string, token: string): Promise<void> {
  await run(["docker", "login", registry, "-u", user, "--password-stdin"], { input: token });
}

/** Tag the built id with the manifest's image ref and push it. */
export async function tagAndPush(iid: string, m: SiteManifest): Promise<string> {
  const ref = imageRef(m);
  await run(["docker", "tag", iid, ref]);
  await run(["docker", "push", ref]);
  return ref;
}
