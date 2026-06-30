import { genSecrets, importSecrets, editSecrets, materialize } from "../lib/sops";
import type { SiteManifest } from "../../sites";

/** secrets gen|import|edit|decrypt for a site. */
export async function secrets(m: SiteManifest, sub: string | undefined): Promise<void> {
  switch (sub) {
    case "gen":
      await genSecrets(m);
      break;
    case "import":
      await importSecrets(m);
      break;
    case "edit":
      await editSecrets(m);
      break;
    case "decrypt":
      await materialize(m);
      console.log(`materialized runtime secrets into ${m.secrets.dir}`);
      break;
    default:
      throw new Error(`secrets subcommand must be gen|import|edit|decrypt (got "${sub ?? ""}")`);
  }
}
