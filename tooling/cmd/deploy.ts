import { build } from "./build";
import { dockerLogin, tagAndPush } from "../lib/docker";
import { resolveSecret } from "../lib/secrets";
import type { SiteManifest } from "../../sites";

/** Build (with verify), then atomically tag+push the SAME image id to GHCR. */
export async function deploy(m: SiteManifest): Promise<void> {
  const iid = await build(m);

  const token = await resolveSecret(m, "ghcr_token");
  if (!token) throw new Error("GHCR token not found (secret ghcr_token or env GHCR_TOKEN)");

  await dockerLogin(m.image.registry, m.image.owner, token);
  const ref = await tagAndPush(iid, m);
  console.log(`pushed ${ref}`);
}
