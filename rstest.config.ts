import { defineConfig } from '@rstest/core';

// Visual / AI-driven desktop tests are slow: model calls and desktop
// interaction each take seconds. Use generous timeouts.
export default defineConfig({
  testTimeout: 180_000,
  hookTimeout: 180_000,
  setupFiles: ['./setup.ts'],
});
