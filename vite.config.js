import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  build: {
    // Preserve console statements in production for debug log system
    // By default, Vite doesn't strip console statements, but we explicitly ensure they're kept
    minify: 'esbuild',
  },
  esbuild: {
    // Ensure console statements are NOT dropped in production builds
    // This allows the debug log system to capture logs even in production
    drop: [], // Empty array means don't drop anything (console, debugger, etc.)
  }
});
