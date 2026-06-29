import { Glob, type BunPlugin } from "bun";
import { cpSync, readFileSync, renameSync, rmSync, writeFileSync, watch } from "fs";

const watchMode = process.argv.includes("--watch");
const projectRoot = process.cwd();

// Force every react / react-dom / react-i18next import AND the design-system /
// pickers packages (with their subpaths, e.g. react/jsx-runtime,
// .../riposte-design-system/theme) to this app's single copy. If any of these
// resolved to more than one copy, hooks break and, worse, the design-system
// ThemeContext would exist twice so `useTheme` could not see the ThemeProvider.
// `react-router-dom`, `react-apexcharts`, etc. are not matched. With the
// committed-dist git deps there is no nesting, so this is mostly insurance, but
// it makes correctness independent of how the packages happen to be installed.
const DEDUPE = /^(react(-dom|-i18next)?|@cavebatsofware\/riposte-(design-system|pickers))(\/.*)?$/;
const dedupeSingletons: BunPlugin = {
  name: "dedupe-singletons",
  setup(build) {
    build.onResolve({ filter: DEDUPE }, (args) => ({
      path: Bun.resolveSync(args.path, projectRoot),
    }));
  },
};

async function buildSocial() {
  rmSync("./social-assets", { recursive: true, force: true });

  const result = await Bun.build({
    entrypoints: ["./social-frontend/index.html"],
    // Hashed JS/CSS land in ./social-assets/app, which main.rs serves at /app,
    // and publicPath matches. The subdir lives in outdir + publicPath, NOT in
    // `naming`, on purpose: with code splitting Bun writes HTML/CSS asset refs
    // as publicPath + the full naming path but inter-chunk JS imports as
    // publicPath + basename, so a `naming` subdir would land in only one of the
    // two (doubling the HTML path or 404-ing the chunk imports). Flat naming
    // keeps basename == naming so both resolve to /app/<file>. index.html is
    // relocated to the social-assets root below, where the SPA handler reads it.
    outdir: "./social-assets/app",
    publicPath: "/app/",
    // Split shared code and per-route `import()`s into their own chunks so the
    // initial payload carries only the shell; heavy route-only deps (the
    // markdown editor on /compose-article, etc.) load when their route does.
    splitting: true,
    naming: {
      asset: "[name]-[hash].[ext]",
      chunk: "[name]-[hash].[ext]",
    },
    // Dev (watch) ships unminified for readable stack traces, matching the old
    // `bun build --watch` behavior; production builds minify.
    minify: !watchMode,
    plugins: [dedupeSingletons],
  });

  if (!result.success) {
    for (const log of result.logs) console.error(log);
    if (!watchMode) process.exit(1);
    return;
  }

  // Rewrite the emitted HTML to work around two Bun HTML+splitting gaps:
  //  1. The module <script> can be injected pointing at a shared "index-*"
  //     chunk instead of the JS entry-point that calls createRoot, orphaning
  //     the real entry so the app never boots. result.outputs marks the
  //     entry-point authoritatively; repointing is a no-op when Bun got it right.
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
    if (!watchMode) process.exit(1);
    return;
  }
  const cssAssets = result.outputs
    .filter((o) => o.kind === "asset" && o.path.endsWith(".css"))
    .map((o) => o.path.split("/").pop() as string)
    .sort((a, b) => Number(b.startsWith("index-")) - Number(a.startsWith("index-")));
  const entryHtml = "./social-assets/app/index.html";
  const html = readFileSync(entryHtml, "utf8")
    .replace(
      /(<script[^>]*type="module"[^>]*src=")[^"]+(")/,
      `$1/app/${entryJs.path.split("/").pop()}$2`
    )
    .replace(
      /<link rel="stylesheet"[^>]*href="\/app\/[^"]*"[^>]*>/,
      cssAssets
        .map((f) => `<link rel="stylesheet" crossorigin href="/app/${f}">`)
        .join("")
    );
  writeFileSync(entryHtml, html);

  // Bun emits index.html beside the assets in outdir; the SPA handler reads it
  // from the social-assets root. Its asset refs are absolute (/app/...), so the
  // move does not disturb them.
  renameSync("./social-assets/app/index.html", "./social-assets/index.html");

  cpSync("social-frontend/public/locales", "social-assets/locales", {
    recursive: true,
  });

  // Precompress for production only; the server's ServeDir uses
  // precompressed_gzip. Dev serves the raw files.
  if (!watchMode) {
    const gzip = (path: string) =>
      writeFileSync(`${path}.gz`, Bun.gzipSync(readFileSync(path)));
    const jsGlob = new Glob("app/**/*.{js,css}");
    for await (const file of jsGlob.scan("./social-assets")) {
      gzip(`social-assets/${file}`);
    }
    const localeGlob = new Glob("**/*.json");
    for await (const file of localeGlob.scan("./social-assets/locales")) {
      gzip(`social-assets/locales/${file}`);
    }
  }
}

await buildSocial();

if (watchMode) {
  console.log("[build-social] watching social-frontend for changes...");
  let timer: ReturnType<typeof setTimeout> | undefined;
  watch("social-frontend", { recursive: true }, () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      buildSocial()
        .then(() => console.log("[build-social] rebuilt"))
        .catch((err) => console.error(err));
    }, 100);
  });
}
