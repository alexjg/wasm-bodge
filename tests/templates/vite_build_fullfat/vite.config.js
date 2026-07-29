import { defineConfig } from 'vite';

// Fullfat loading intentionally uses no Wasm plugin and does not exclude the
// package from dependency optimization. The package's static URL must work in
// an ordinary Vite 8 project.
export default defineConfig({
  // Relative base verifies that the emitted Wasm URL is not tied to `/`.
  base: './',
  build: { target: 'esnext' },
});
