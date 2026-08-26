export interface OxcTsrxToolchain {
  readonly name: "@tsrx/oxc";
  readonly language: "tsrx";
  readonly extensions: readonly [".tsrx"];
  readonly capabilities: readonly ["parser", "lint", "format", "languageServer"];
}

export declare const toolchain: Readonly<OxcTsrxToolchain>;
