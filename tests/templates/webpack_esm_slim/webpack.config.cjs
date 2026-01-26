const path = require('path');
const CopyPlugin = require('copy-webpack-plugin');

module.exports = {
  entry: './main.js',
  output: {
    filename: 'bundle.js',
    path: path.resolve(__dirname, 'dist'),
  },
  experiments: {
    asyncWebAssembly: true,
  },
  plugins: [
    new CopyPlugin({
      patterns: [{ from: 'index.html', to: 'index.html' }],
    }),
  ],
  mode: 'production',
};
