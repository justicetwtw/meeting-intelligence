import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: [
      "apps/**/*.test.{ts,tsx}",
      "packages/**/*.test.{ts,tsx}",
      "slack/**/*.test.{ts,tsx}",
    ],
    setupFiles: ["./apps/desktop/src/test/setup.ts"],
    restoreMocks: true,
  },
});
