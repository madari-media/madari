import {defineConfig} from 'vite';
import react from '@vitejs/plugin-react';
import {astryxStylex} from '@astryxdesign/build/vite';

// The Rust server embeds the build output by fixed path (web/dist), so the
// bundle must be a single file with no content hashes: anything split out
// (a lazy chunk, a hashed asset) would be requested from a route that does not
// exist. Fonts are inlined for the same reason.
export default defineConfig({
  plugins: [...astryxStylex(), react()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // One CSS file so the Rust server can embed it at a fixed path.
    cssCodeSplit: false,
    assetsInlineLimit: 1024 * 1024,
    rolldownOptions: {
      output: {
        // Astryx lazily imports a few small helpers; inline them so the single
        // embedded app.js is the whole application.
        codeSplitting: false,
        entryFileNames: 'app.js',
        assetFileNames: (assetInfo) =>
          assetInfo.names?.some((name) => name.endsWith('.css')) ? 'style.css' : 'assets/[name][extname]',
      },
    },
  },
});
