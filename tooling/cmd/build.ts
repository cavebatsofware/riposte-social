import { shopBuild } from "./shop-build";
import { dockerBuild, verifyBaked } from "../lib/docker";
import type { SiteManifest } from "../../sites";

/** Stage the shop (if any), build the image, verify baked identity. Returns the image id. */
export async function build(m: SiteManifest): Promise<string> {
  if (m.shop) await shopBuild(m);
  const iid = await dockerBuild(m);
  await verifyBaked(iid, m);
  return iid;
}
