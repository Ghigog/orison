import { defineConfig } from "vitest/config";

// jsdom, not node: markdown.ts sanitises through DOMPurify and returns a
// DocumentFragment, so the tests have to assert against a real DOM. They
// check what the player would be shown, not what the parser said.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
