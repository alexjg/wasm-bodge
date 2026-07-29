import { defineConfig } from 'vite';

// Vite 8 remaps relative asset URLs when it optimizes dependencies, so no
// Wasm plugin or dependency-optimizer exclusion is needed.
export default defineConfig({
  build: { target: 'esnext' },
});
