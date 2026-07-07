import { run } from "../lib/proc";
import { waitForHttp } from "../lib/http";
import { repoRoot } from "../lib/manifest";

// Suite -> the package.json script that runs it in the cypress/included image.
const SUITES: Record<string, string> = {
  feature: "e2e:feature:docker",
  a11y: "a11y:smoke:docker",
  "a11y:strict": "a11y:smoke:strict:docker",
  share: "e2e:share:docker",
  screens: "e2e:screens:docker",
  all: "e2e:docker",
};

/**
 * Bring up the containerized test stack (postgres-test + minio-test + app-test
 * on :3001), wait for the app, then run a Cypress suite against it. Collapses
 * the make test-app-up + bun run e2e:*:docker sequence into one verb. The stack
 * is left running so specs can be re-run and failures inspected; tear it down
 * with `docker compose -f docker-compose.test.yml --profile app down`. The test
 * stack is site-agnostic (app-test carries its own fixed test env), so no site
 * manifest is needed.
 */
export async function cypress(sub: string | undefined): Promise<void> {
  const suite = sub ?? "all";
  const script = SUITES[suite];
  if (!script) {
    throw new Error(`cypress subcommand must be ${Object.keys(SUITES).join("|")} (got "${sub ?? ""}")`);
  }

  const root = repoRoot();
  const compose = ["docker", "compose", "-f", "docker-compose.test.yml", "--profile", "app"];
  await run([...compose, "up", "-d", "--build"], { cwd: root });
  await waitForHttp("http://localhost:3001/health");

  const env = {
    CYPRESS_BASE_URL: "http://localhost:3001",
    TEST_ADMIN_EMAIL: process.env.TEST_ADMIN_EMAIL ?? "admin@test.local",
    TEST_ADMIN_PASSWORD: process.env.TEST_ADMIN_PASSWORD ?? "test_admin_password",
  };
  await run(["bun", "run", script], { cwd: root, env });
}
