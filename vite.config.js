import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import compression from "vite-plugin-compression";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");

  return {
    plugins: [react(), compression({ algorithm: "gzip" })],
    root: "admin-frontend",
    base: "/admin/",
    build: {
      outDir: "../admin-assets",
      emptyOutDir: true,
      minify: true,
    },
    server: {
      port: 5173,
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
