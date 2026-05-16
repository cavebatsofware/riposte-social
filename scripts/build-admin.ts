import { Glob } from "bun";
import { readFileSync, rmSync, writeFileSync } from "fs";

rmSync("./admin-assets", { recursive: true, force: true });

const result = await Bun.build({
  entrypoints: ["./admin-frontend/index.html"],
  outdir: "./admin-assets",
  publicPath: "/admin/",
  naming: {
    asset: "assets/[name]-[hash].[ext]",
    chunk: "assets/[name]-[hash].[ext]",
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

const glob = new Glob("assets/**/*.{js,css}");
for await (const file of glob.scan("./admin-assets")) {
  writeFileSync(`admin-assets/${file}.gz`, Bun.gzipSync(readFileSync(`admin-assets/${file}`)));
}
