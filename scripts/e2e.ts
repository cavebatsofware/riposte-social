#!/usr/bin/env bun
/// End-to-end test orchestrator. Owns the full lifecycle so bun is the
/// single entry point: it brings up the containerized test stack
/// (postgres + minio + the app-test container from
/// docker-compose.test.yml), waits for the app health check, then runs
/// Cypress against it in the pinned cypress/included container. This
/// replaces the Makefile's test-app-up + cypress-* targets.
///
/// Usage:
///   bun run scripts/e2e.ts [--spec <group>] [--no-build] [--down]
///   bun run scripts/e2e.ts --up-only        # start the stack, no tests
///   bun run scripts/e2e.ts --down-only       # stop the stack and exit
///
/// Spec groups (see SPEC_GROUPS): omit --spec (or pass "all") to run every
/// spec under cypress/e2e, so new specs are picked up automatically.
///
/// The stack is left running after a test run (matching the previous
/// Makefile behavior) so reruns are fast; pass --down to stop it, or run
/// with --down-only.

const COMPOSE_FILE = "docker-compose.test.yml";
const CYPRESS_IMAGE = "cypress/included:15.14.2";
const BASE_URL = process.env.CYPRESS_BASE_URL ?? "http://localhost:3001";
const HEALTH_URL = `${BASE_URL}/health`;
const HEALTH_TIMEOUT_MS = 120_000;

/// Friendly spec-group names mapped to Cypress --spec globs. "all" omits
/// the flag entirely so the run covers everything cypress.config.js
/// discovers.
const SPEC_GROUPS: Record<string, string> = {
  feature: "cypress/e2e/feed_kinds.cy.js,cypress/e2e/post_kind_routing.cy.js",
  share: "cypress/e2e/share_og.cy.js,cypress/e2e/share_menu.cy.js",
  a11y: "cypress/e2e/a11y.cy.js",
};

/// Glob for the opt-in screenshot capture specs. They live outside
/// cypress/e2e/ so the normal suite never runs them; selecting the
/// "screens" group overrides specPattern to reach them on demand.
const SCREENS_PATTERN = "cypress/screens/**/*.cy.js";

interface Options {
  spec: string;
  build: boolean;
  downAfter: boolean;
  upOnly: boolean;
  downOnly: boolean;
  reset: boolean;
  logs: boolean;
  strict: boolean;
}

function parseArgs(argv: string[]): Options {
  const opts: Options = {
    spec: "all",
    build: true,
    downAfter: false,
    upOnly: false,
    downOnly: false,
    reset: false,
    logs: false,
    strict: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case "--spec":
        opts.spec = argv[++i];
        break;
      case "--no-build":
        opts.build = false;
        break;
      case "--down":
        opts.downAfter = true;
        break;
      case "--up-only":
        opts.upOnly = true;
        break;
      case "--down-only":
        opts.downOnly = true;
        break;
      case "--reset":
        opts.reset = true;
        break;
      case "--logs":
        opts.logs = true;
        break;
      case "--strict":
        opts.strict = true;
        break;
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }
  if (
    opts.spec !== "all" &&
    opts.spec !== "screens" &&
    !(opts.spec in SPEC_GROUPS)
  ) {
    throw new Error(
      `Unknown spec group "${opts.spec}". Known: all, screens, ${Object.keys(SPEC_GROUPS).join(", ")}`,
    );
  }
  return opts;
}

/// Run a command with inherited stdio and resolve to its exit code.
async function run(cmd: string[], env?: Record<string, string>): Promise<number> {
  const proc = Bun.spawn(cmd, {
    stdout: "inherit",
    stderr: "inherit",
    stdin: "inherit",
    env: env ? { ...process.env, ...env } : process.env,
  });
  return await proc.exited;
}

async function composeUp(build: boolean): Promise<void> {
  console.log("🚀 Starting test stack (postgres + minio + app)...");
  const cmd = [
    "docker",
    "compose",
    "-f",
    COMPOSE_FILE,
    "--profile",
    "app",
    "up",
    "-d",
  ];
  if (build) cmd.push("--build");
  const code = await run(cmd);
  if (code !== 0) {
    throw new Error(`docker compose up failed (exit ${code})`);
  }
}

async function composeDown(volumes: boolean): Promise<void> {
  console.log(volumes ? "🗑️  Resetting test stack (dropping volumes)..." : "🛑 Stopping test stack...");
  const cmd = ["docker", "compose", "-f", COMPOSE_FILE, "--profile", "app", "down"];
  if (volumes) cmd.push("-v");
  await run(cmd);
}

async function composeLogs(): Promise<void> {
  await run([
    "docker",
    "compose",
    "-f",
    COMPOSE_FILE,
    "--profile",
    "app",
    "logs",
    "-f",
    "app-test",
  ]);
}

async function waitForHealth(): Promise<void> {
  console.log(`⏳ Waiting for ${HEALTH_URL} ...`);
  const deadline = Date.now() + HEALTH_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(HEALTH_URL);
      if (res.ok) {
        console.log("✅ Test app is up.");
        return;
      }
    } catch {
      // Connection refused until the app binds; keep polling.
    }
    await Bun.sleep(2000);
  }
  throw new Error(
    `Test app did not become healthy within ${HEALTH_TIMEOUT_MS / 1000}s. See: docker logs riposte-social-test-app`,
  );
}

async function runCypress(opts: Options): Promise<number> {
  const cmd = [
    "docker",
    "run",
    "--rm",
    "--network=host",
    "-e",
    `CYPRESS_BASE_URL=${BASE_URL}`,
    "-e",
    "TEST_ADMIN_EMAIL",
    "-e",
    "TEST_ADMIN_PASSWORD",
    "-e",
    "CYPRESS_VIDEO",
    "-e",
    "CYPRESS_SCREENSHOTS",
  ];
  if (opts.strict) {
    cmd.push("-e", "A11Y_STRICTNESS=strict");
  }
  cmd.push(
    "-v",
    `${process.cwd()}:/e2e`,
    "-w",
    "/e2e",
    CYPRESS_IMAGE,
    "cypress",
    "run",
  );
  if (opts.spec === "screens") {
    // The capture specs sit outside the default specPattern, so point
    // Cypress at them explicitly for this run only.
    cmd.push("--config", `specPattern=${SCREENS_PATTERN}`);
  } else if (opts.spec !== "all") {
    cmd.push("--spec", SPEC_GROUPS[opts.spec]);
  }
  const label = opts.spec === "all" ? "all specs" : `${opts.spec} specs`;
  console.log(`🧪 Running Cypress (${label})...`);
  return await run(cmd);
}

async function main(): Promise<void> {
  const opts = parseArgs(Bun.argv.slice(2));

  if (opts.logs) {
    await composeLogs();
    return;
  }

  if (opts.downOnly) {
    await composeDown(false);
    return;
  }

  // --reset drops the postgres volume for a deterministic seed/migration
  // starting point before the stack comes back up.
  if (opts.reset) {
    await composeDown(true);
  }

  await composeUp(opts.build);
  await waitForHealth();

  if (opts.upOnly) {
    console.log(`✅ Stack ready at ${BASE_URL}. Skipping tests (--up-only).`);
    return;
  }

  const code = await runCypress(opts);

  if (opts.downAfter) {
    await composeDown(false);
  }

  if (code !== 0) {
    console.error(`❌ Cypress failed (exit ${code}).`);
    process.exit(code);
  }
  console.log("✅ E2E suite passed.");
}

await main();
