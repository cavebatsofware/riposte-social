import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { run, capture } from "./proc";
import { repoRoot } from "./manifest";
import { resolveSecret } from "./secrets";
import type { SiteManifest } from "../../sites";

const COMPOSE = ["docker", "compose", "-f", "docker-compose.prod.yml"];

function readEnvVar(file: string, key: string): string | undefined {
  if (!existsSync(file)) return undefined;
  for (const line of readFileSync(file, "utf8").split("\n")) {
    const m = line.match(new RegExp(`^\\s*${key}=(.*)$`));
    if (m) return m[1].trim();
  }
  return undefined;
}

// Idempotent: create the role or re-sync its password from the secret, then
// create the database only if absent (\gexec, since CREATE DATABASE cannot run
// inside the DO block / a transaction). psql vars are passed with -v; :'x'
// expands to a quoted string literal that format() then renders as %I / %L.
const SQL = `DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'role') THEN
    EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', :'role', :'pw');
  ELSE
    EXECUTE format('ALTER ROLE %I LOGIN PASSWORD %L', :'role', :'pw');
  END IF;
END $$;
SELECT format('CREATE DATABASE %I OWNER %I', :'dbname', :'role')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'dbname')
\\gexec
`;

/** Poll the container's pg_isready until Postgres accepts connections (or time out). */
export async function waitForPostgres(
  compose: string[],
  service: string,
  cwd: string,
  timeoutMs = 60_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const res = await capture([...compose, "exec", "-T", service, "pg_isready"], { cwd });
    if (res.code === 0) return;
    if (Date.now() > deadline) {
      throw new Error(`postgres (${service}) not ready after ${timeoutMs}ms`);
    }
    await Bun.sleep(500);
  }
}

/**
 * Ensure the site's login role and database exist on the shared prod postgres,
 * re-syncing the role password to the postgres_password secret. Connects as the
 * stack superuser (POSTGRES_USER from deploy/.env.prod) inside the container.
 */
export async function ensureRoleAndDb(m: SiteManifest): Promise<void> {
  const root = repoRoot();
  const superuser = readEnvVar(join(root, "deploy/.env.prod"), "POSTGRES_USER");
  if (!superuser) {
    throw new Error("could not read POSTGRES_USER from deploy/.env.prod (the provisioning superuser)");
  }
  const pw = await resolveSecret(m, "postgres_password");
  if (!pw) throw new Error(`postgres_password not found for ${m.name} (run: secrets gen/edit ${m.name})`);

  const psql = [
    ...COMPOSE, "exec", "-T", "postgres",
    "psql", "-v", "ON_ERROR_STOP=1", "-U", superuser, "-d", "postgres",
    "-v", `role=${m.db.role}`, "-v", `dbname=${m.db.database}`, "-v", `pw=${pw}`,
  ];
  await run(psql, { cwd: root, input: SQL });
}
