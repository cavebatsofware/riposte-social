import type { SiteManifest } from "./manifest";
import { randhillwoodworks } from "./randhillwoodworks";
import { cavebatsoftware } from "./cavebatsoftware";
import { grant } from "./grant";

/** Every deployable site, keyed by manifest name. */
export const SITES: Record<string, SiteManifest> = {
  [randhillwoodworks.name]: randhillwoodworks,
  [cavebatsoftware.name]: cavebatsoftware,
  [grant.name]: grant,
};

export type { SiteManifest } from "./manifest";
export { validateManifest, imageRef } from "./manifest";
