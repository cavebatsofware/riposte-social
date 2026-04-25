import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import compression from "vite-plugin-compression";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");

  return {
    plugins: [react(), compression({ algorithm: "gzip" })],
    root: "social-frontend",
    base: "/",
    build: {
      outDir: "../social-assets",
      emptyOutDir: true,
      minify: true,
      // bundles land under /app/* instead of /assets/* so they don't
      // collide with the template's /assets ServeDir for document icons.
      assetsDir: "app",
    },
    server: {
      port: 5174,
      watch: {
        usePolling: true,
        interval: 100,
      },
      proxy: {
        "/api": {
          target: "http://localhost:3000",
          changeOrigin: true,
        },
      },
    },
    define: {
      "import.meta.env.VITE_SITE_DOMAIN": JSON.stringify(env.SITE_DOMAIN),
    },
  };
});
