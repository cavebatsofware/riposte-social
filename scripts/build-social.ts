import { Glob } from "bun";
import { cpSync, rmSync } from "fs";

rmSync("./social-assets", { recursive: true, force: true });

const result = await Bun.build({
  entrypoints: ["./social-frontend/index.html"],
  outdir: "./social-assets",
  publicPath: "/",
  naming: {
    asset: "app/[name]-[hash].[ext]",
    chunk: "app/[name]-[hash].[ext]",
  },
  minify: true,
  define: {
    "import.meta.env.VITE_SITE_DOMAIN": JSON.stringify(
      process.env.SITE_DOMAIN ?? ""
    ),
  },
});

if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}

cpSync("social-frontend/public/locales", "social-assets/locales", {
  recursive: true,
});

const jsGlob = new Glob("app/**/*.{js,css}");
for await (const file of jsGlob.scan("./social-assets")) {
  const proc = Bun.spawn(["gzip", "-k", "-f", `social-assets/${file}`], {
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await proc.exited;
  if (code !== 0) {
    console.error(`gzip failed (exit ${code}): social-assets/${file}`);
    process.exit(code);
  }
}

const localeGlob = new Glob("**/*.json");
for await (const file of localeGlob.scan("./social-assets/locales")) {
  const proc = Bun.spawn(["gzip", "-k", "-f", `social-assets/locales/${file}`], {
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await proc.exited;
  if (code !== 0) {
    console.error(`gzip failed (exit ${code}): social-assets/${file}`);
    process.exit(code);
  }
}
