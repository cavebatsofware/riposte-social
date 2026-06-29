import { run } from "../lib/proc";
import { repoRoot } from "../lib/manifest";
import { featuresFlag } from "../lib/site-env";
import type { SiteManifest } from "../../sites";

/** Bring up the test DB and run cargo tests against it (e2e_testing + business). */
export async function test(m: SiteManifest): Promise<void> {
  const root = repoRoot();
  await run(["docker", "compose", "-f", "docker-compose.test.yml", "up", "-d"], { cwd: root });

  const user = process.env.TEST_POSTGRES_USER ?? "riposte_social_test_user";
  const pw = process.env.TEST_POSTGRES_PASSWORD ?? "test_password";
  const port = process.env.TEST_POSTGRES_PORT ?? "5433";
  const db = process.env.TEST_POSTGRES_DB ?? "riposte_social_test";
  const url = `postgresql://${user}:${pw}@localhost:${port}/${db}`;

  const env = {
    DATABASE_URL: url,
    TEST_DATABASE_URL: url,
    SECURE_VALUES_KEY: process.env.SECURE_VALUES_KEY ?? "0".repeat(64),
  };
  await run(["cargo", "test", ...featuresFlag(m, ["e2e_testing"])], { cwd: root, env });
}
