import { Glob, type BunPlugin } from "bun";
import { readFileSync, renameSync, rmSync, writeFileSync } from "fs";

const projectRoot = process.cwd();

// Force every react / react-dom / react-i18next import AND the design-system /
// pickers packages (with their subpaths) to this app's single copy. Without it,
// the design-system ThemeContext could exist twice so `useTheme` would not see
// the ThemeProvider. Mirrors build-social.ts; required now that admin consumes
// the design-system theme engine + components.
const DEDUPE = /^(react(-dom|-i18next)?|@cavebatsofware\/riposte-(design-system|pickers))(\/.*)?$/;
const dedupeSingletons: BunPlugin = {
  name: "dedupe-singletons",
  setup(build) {
    build.onResolve({ filter: DEDUPE }, (args) => ({
      path: Bun.resolveSync(args.path, projectRoot),
    }));
  },
};

rmSync("./admin-assets", { recursive: true, force: true });

const result = await Bun.build({
  entrypoints: ["./admin-frontend/index.html"],
  // Hashed JS/CSS land in ./admin-assets/assets, which main.rs serves at
  // /admin/assets, and publicPath matches. The subdir lives in outdir +
  // publicPath, NOT in `naming`: with code splitting Bun writes HTML/CSS asset
  // refs as publicPath + the full naming path but inter-chunk JS imports as
  // publicPath + basename, so a `naming` subdir would land in only one of the
  // two (doubling the HTML path or 404-ing the chunk imports). Flat naming keeps
  // basename == naming so both resolve to /admin/assets/<file>. index.html is
  // relocated to the admin-assets root below, where the SPA handler reads it.
  outdir: "./admin-assets/assets",
  publicPath: "/admin/assets/",
  // Split shared code and per-route `import()`s into their own chunks so the
  // login/initial payload carries only the shell; route-only deps (ApexCharts
  // on the dashboard, etc.) load when their route does.
  splitting: true,
  naming: {
    asset: "[name]-[hash].[ext]",
    chunk: "[name]-[hash].[ext]",
  },
  minify: true,
  plugins: [dedupeSingletons],
});

if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}

// Rewrite the emitted HTML to work around two Bun HTML+splitting gaps:
//  1. The module <script> can be injected pointing at a shared "index-*" chunk
//     instead of the JS entry-point that calls createRoot, orphaning the real
//     entry so the app never boots. result.outputs marks the entry-point
//     authoritatively; repointing from it is a no-op when Bun got it right.
//  2. Per-route CSS (imported by lazy chunks) is extracted into its own asset
//     but never wired to load, so those routes render unstyled. Link every CSS
//     asset in the head (global bundle first). Feature CSS is class-scoped, so
//     eager-loading it is inert elsewhere, and the pre-split bundle shipped all
//     CSS up front anyway.
const entryJs = result.outputs.find(
  (o) => o.kind === "entry-point" && o.path.endsWith(".js")
);
if (!entryJs) {
  console.error("build produced no JS entry-point");
  process.exit(1);
}
const cssAssets = result.outputs
  .filter((o) => o.kind === "asset" && o.path.endsWith(".css"))
  .map((o) => o.path.split("/").pop() as string)
  .sort((a, b) => Number(b.startsWith("index-")) - Number(a.startsWith("index-")));
const entryHtml = "./admin-assets/assets/index.html";
const html = readFileSync(entryHtml, "utf8")
  .replace(
    /(<script[^>]*type="module"[^>]*src=")[^"]+(")/,
    `$1/admin/assets/${entryJs.path.split("/").pop()}$2`
  )
  .replace(
    /<link rel="stylesheet"[^>]*href="\/admin\/assets\/[^"]*"[^>]*>/,
    cssAssets
      .map((f) => `<link rel="stylesheet" crossorigin href="/admin/assets/${f}">`)
      .join("")
  );
writeFileSync(entryHtml, html);

// Bun emits index.html beside the assets in outdir; the SPA handler reads it
// from the admin-assets root. Its asset refs are absolute (/admin/assets/...),
// so the move does not disturb them.
renameSync(entryHtml, "./admin-assets/index.html");

const glob = new Glob("assets/**/*.{js,css}");
for await (const file of glob.scan("./admin-assets")) {
  writeFileSync(`admin-assets/${file}.gz`, Bun.gzipSync(readFileSync(`admin-assets/${file}`)));
}
