const extensions = Object.freeze([".tsrx"]);
const capabilities = Object.freeze(["parser", "lint", "format", "languageServer"]);

export const toolchain = Object.freeze({
  name: "@tsrx/oxc",
  language: "tsrx",
  extensions,
  capabilities,
});
