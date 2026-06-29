import { run } from "../lib/proc";
import { repoRoot } from "../lib/manifest";
import { devEnv, featuresFlag } from "../lib/site-env";
import type { SiteManifest } from "../../sites";

/** Dev database management against the local docker-compose postgres. */
export async function db(m: SiteManifest, sub: string | undefined): Promise<void> {
  const root = repoRoot();
  const env = { ...devEnv(m), SITE_DOMAIN: m.siteDomain };
  const compose = (args: string[]) => run(["docker", "compose", ...args], { cwd: root, env });
  const user = env.POSTGRES_USER ?? "riposte_social_user";
  const name = env.POSTGRES_DB ?? "riposte_social";
  const migrate = () =>
    run(["cargo", "run", ...featuresFlag(m), "--", "migrate"], { cwd: root, env: { ...env, MIGRATE_DB: "true" } });

  switch (sub) {
    case "up":
      await compose(["up", "-d", "postgres"]);
      break;
    case "down":
      await compose(["down"]);
      break;
    case "logs":
      await compose(["logs", "-f", "postgres"]);
      break;
    case "shell":
      await compose(["exec", "postgres", "psql", "-U", user, "-d", name]);
      break;
    case "migrate":
      await migrate();
      break;
    case "reset":
      await compose(["down", "-v"]);
      await compose(["up", "-d", "postgres"]);
      await migrate();
      break;
    default:
      throw new Error(`db subcommand must be up|down|logs|shell|migrate|reset (got "${sub ?? ""}")`);
  }
}
