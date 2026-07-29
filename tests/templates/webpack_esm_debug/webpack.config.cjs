const path = require('path');
const CopyPlugin = require('copy-webpack-plugin');

module.exports = {
  entry: './main.js',
  output: {
    filename: 'bundle.js',
    path: path.resolve(__dirname, 'dist'),
    // Relative public path verifies deployment below a non-root URL.
    publicPath: './',
    assetModuleFilename: 'assets/[name].[contenthash][ext]',
  },
  experiments: {
    asyncWebAssembly: true,
    // Required for Webpack < 5.83; harmless on newer Webpack 5 releases.
    topLevelAwait: true,
  },
  plugins: [
    new CopyPlugin({
      patterns: [{ from: 'index.html', to: 'index.html' }],
    }),
  ],
  mode: 'production',
};
