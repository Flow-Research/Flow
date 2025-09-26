import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  base: "./", // Important for Electron
  server: {
    port: 3000,
    open: false, // Don't open browser in Electron mode
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: undefined,
      },
    },
  },
  // Optimize for Electron
  optimizeDeps: {
    exclude: ["electron"],
  },
});
