import { defineConfig } from "vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // M18: dos puntos de entrada. `preview.html` es la ventana de vista previa desacoplada;
  // se mantiene aparte de index.html para que NO arrastre el bundle del editor (CodeMirror,
  // fuentes, spellcheck) — solo necesita un iframe y un listener.
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        preview: "preview.html",
      },
    },
  },
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      // El workspace de cargo deja `target/` en la raíz: si Vite lo vigila, en Windows
      // choca con la .dll que cargo está escribiendo (EBUSY) y se cae. `crates/` es Rust
      // puro; tampoco tiene nada que recargar.
      ignored: ["**/src-tauri/**", "**/target/**", "**/crates/**"],
    },
  },
}));
