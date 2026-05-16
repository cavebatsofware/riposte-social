import { Glob } from "bun";
import { rmSync } from "fs";

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
  const proc = Bun.spawn(["gzip", "-k", "-f", `admin-assets/${file}`], {
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await proc.exited;
  if (code !== 0) {
    console.error(`gzip failed (exit ${code}): admin-assets/${file}`);
    process.exit(code);
  }
}
