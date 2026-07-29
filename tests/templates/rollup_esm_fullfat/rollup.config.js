import { readFileSync } from 'node:fs';
import { nodeResolve } from '@rollup/plugin-node-resolve';
import { importMetaAssets } from '@web/rollup-plugin-import-meta-assets';

export default {
  input: 'main.js',
  output: {
    file: 'dist/bundle.js',
    format: 'es',
  },
  plugins: [
    nodeResolve({ browser: true }),
    importMetaAssets(),
    {
      name: 'copy-test-html',
      generateBundle() {
        this.emitFile({
          type: 'asset',
          fileName: 'index.html',
          source: readFileSync('index.html'),
        });
      },
    },
  ],
};
