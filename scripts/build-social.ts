import { Glob } from "bun";
import { cpSync } from "fs";

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
  await proc.exited;
}

const localeGlob = new Glob("**/*.json");
for await (const file of localeGlob.scan("./social-assets/locales")) {
  const proc = Bun.spawn(["gzip", "-k", "-f", `social-assets/locales/${file}`], {
    stdout: "inherit",
    stderr: "inherit",
  });
  await proc.exited;
}
