import { Glob, type BunPlugin } from "bun";
import { cpSync, readFileSync, rmSync, writeFileSync, watch } from "fs";

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
    outdir: "./social-assets",
    publicPath: "/",
    naming: {
      asset: "app/[name]-[hash].[ext]",
      chunk: "app/[name]-[hash].[ext]",
    },
    // Dev (watch) ships unminified for readable stack traces, matching the old
    // `bun build --watch` behavior; production builds minify.
    minify: !watchMode,
    plugins: [dedupeSingletons],
    define: {
      "import.meta.env.VITE_SITE_DOMAIN": JSON.stringify(
        process.env.SITE_DOMAIN ?? ""
      ),
    },
  });

  if (!result.success) {
    for (const log of result.logs) console.error(log);
    if (!watchMode) process.exit(1);
    return;
  }

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
