import { materialize } from "../lib/sops";
import { ensureRoleAndDb, waitForPostgres } from "../lib/sql";
import { run } from "../lib/proc";
import { repoRoot } from "../lib/manifest";
import type { SiteManifest } from "../../sites";

/**
 * Bring a site online from scratch (idempotent): materialize its secrets, make
 * sure the shared postgres is up, ensure its DB role and database exist, then
 * start its compose service. MIGRATE_DB runs SeaORM migrations on boot.
 */
export async function provision(m: SiteManifest): Promise<void> {
  const root = repoRoot();
  await materialize(m);
  await run(["docker", "compose", "-f", "docker-compose.prod.yml", "up", "-d", "postgres"], { cwd: root });
  await waitForPostgres(["docker", "compose", "-f", "docker-compose.prod.yml"], "postgres", root);
  await ensureRoleAndDb(m);
  await run(["docker", "compose", "-f", "docker-compose.prod.yml", "up", "-d", m.compose.service], { cwd: root });
  console.log(`provisioned ${m.name}: secrets materialized, role/db ensured, ${m.compose.service} up`);
}
