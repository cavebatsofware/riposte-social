import type { SiteManifest } from "./manifest";
import { randhillwoodworks } from "./randhillwoodworks";
import { cavebatsoftware } from "./cavebatsoftware";

/** Every deployable site, keyed by manifest name. */
export const SITES: Record<string, SiteManifest> = {
  [randhillwoodworks.name]: randhillwoodworks,
  [cavebatsoftware.name]: cavebatsoftware,
};

export type { SiteManifest } from "./manifest";
export { validateManifest, imageRef } from "./manifest";
