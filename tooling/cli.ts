#!/usr/bin/env bun
/**
 * riposte-social tooling CLI. Single interface and task runner for a site,
 * selected by its manifest name in sites/.
 *
 *   bun tooling/cli.ts <verb> <site> [sub]
 *   bun tooling/cli.ts build cavebatsoftware
 *   bun tooling/cli.ts deploy cavebatsoftware
 *   bun tooling/cli.ts provision cavebatsoftware
 *   bun tooling/cli.ts secrets cavebatsoftware edit
 *   bun tooling/cli.ts db randhillwoodworks up
 */
import { SITES, imageRef } from "../sites";
import { loadSite } from "./lib/manifest";
import { dockerBuild, verifyBaked } from "./lib/docker";
import { build } from "./cmd/build";
import { deploy } from "./cmd/deploy";
import { shopBuild } from "./cmd/shop-build";
import { provision } from "./cmd/provision";
import { secrets } from "./cmd/secrets";
import { dev } from "./cmd/dev";
import { test } from "./cmd/test";
import { db } from "./cmd/db";

function usage(): void {
  console.log(`riposte-social tooling

Usage: bun tooling/cli.ts <verb> <site> [sub]

Build / deploy:
  build <site>          shop-build (if any) + docker build + verify baked identity
  shop-build <site>     build + stage the storefront export into shop-assets/
  deploy <site>         build (with verify) + atomic tag/push to GHCR
  verify <site>         build then re-check the shop's baked Turnstile key
  provision <site>      decrypt secrets + ensure DB role/db + bring the service up

Secrets:
  secrets <site> gen     create the encrypted secrets file (NEW site; generated values)
  secrets <site> import  create it from existing plaintext (safe for a live deployment)
  secrets <site> edit    open it decrypted in \$EDITOR
  secrets <site> decrypt materialize runtime secrets for compose

Dev:
  dev <site>            db up + build SPAs + watch all three
  test <site>           test DB up + cargo test
  db <site> <sub>       up | down | logs | shell | migrate | reset

  show <site>           print the resolved, validated manifest
  sites                 list known sites

Sites: ${Object.keys(SITES).join(", ")}`);
}

async function main(): Promise<void> {
  const [verb, siteName, sub] = process.argv.slice(2);

  if (!verb || verb === "help" || verb === "-h" || verb === "--help") {
    usage();
    return;
  }
  if (verb === "sites" || verb === "list") {
    console.log(Object.keys(SITES).join("\n"));
    return;
  }
  if (!siteName) {
    throw new Error(`verb "${verb}" needs a <site>. Known: ${Object.keys(SITES).join(", ")}`);
  }

  const m = loadSite(siteName);

  switch (verb) {
    case "show":
      console.log(JSON.stringify(m, null, 2));
      break;
    case "shop-build":
      await shopBuild(m);
      break;
    case "build": {
      const iid = await build(m);
      console.log(`built ${iid} (${imageRef(m)})`);
      break;
    }
    case "deploy":
      await deploy(m);
      break;
    case "verify": {
      const iid = await dockerBuild(m);
      await verifyBaked(iid, m);
      console.log(`verify ok: ${m.name}`);
      break;
    }
    case "provision":
      await provision(m);
      break;
    case "secrets":
      await secrets(m, sub);
      break;
    case "dev":
      await dev(m);
      break;
    case "test":
      await test(m);
      break;
    case "db":
      await db(m, sub);
      break;
    default:
      throw new Error(`unknown verb "${verb}". Run with no args for usage.`);
  }
}

main().catch((e) => {
  console.error(`error: ${e instanceof Error ? e.message : String(e)}`);
  process.exit(1);
});
