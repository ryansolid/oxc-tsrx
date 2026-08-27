# website-oxc

Vercel project root for the docs site at **https://oxc.tsrx.dev**, following the
`website-tsrx` convention in [tsrx-org/tsrx](https://github.com/tsrx-org/tsrx/tree/main/website-tsrx).

The site itself is generated from `../docs` by `docs/build.mjs`. This folder holds
no content — `vercel.json` drives the build: install the workspace at the repo
root, run the docs generator with `SITE_ORIGIN=https://oxc.tsrx.dev SITE_BASE=/`
(the same generator that publishes https://compiled.run/oxc-tsrx, just rooted at
`/`), and serve the output from `dist/`.

## One-time Vercel setup

1. Create a Vercel project from this repository with **Root Directory: `website-oxc`**
   and Framework Preset **Other**. `vercel.json` supplies the install/build/output
   settings.
2. Add the domain **oxc.tsrx.dev** to the project (DNS per Vercel's instructions).
3. Deploy. No repository secrets or variables are required for this path.

## The playground's WASM demo engine

The playground can run the real lint, format, and projection engines in the
browser, compiled to WebAssembly. Vercel's builder cannot produce that artifact:
it has no cargo and no `wasm32-wasip1-threads` target, so `pnpm run docs:wasm`
is not an option here. The engine reaches this build as prebuilt bytes instead.

1. `.github/workflows/site-artifact.yml` builds the engine on GitHub Actions and
   proves it — the site build runs with `OXC_TSRX_REQUIRE_WASM=1` and the static
   verifier runs with `--require-wasm`, so nothing unproven is ever published.
2. On `main`, that workflow uploads the two files the engine needs
   (`demo-wasm.wasm` and `wasi-worker-browser.mjs`) to the rolling
   [`wasm-demo-latest`](https://github.com/tsrx-org/oxc/releases/tag/wasm-demo-latest)
   release, which is a delivery channel and not something to install.
3. It then writes `wasm-pin.json` here — the release tag, a hash of the sources
   the engine was compiled from, and the sha256 and byte count of each file —
   and commits it only when that content changed. That commit is also the
   trigger: a push by `GITHUB_TOKEN` starts no further Actions run, but it does
   reach Vercel's webhook, so the pin and the deploy that consumes it arrive
   together.
4. `pnpm run build` here runs `../scripts/fetch-docs-wasm.ts` first, which reads
   the pin, recomputes the source hash against this checkout, downloads both
   files into `docs/tools/demo-wasm/dist/`, and verifies them byte for byte.

The pin is what keeps this honest. The hash covers `docs/tools/demo-wasm`, the
`tsrx_format`, `tsrx_lint`, and `tsrx_syntax` crates, and `rust-toolchain.toml`,
computed by the same code in `scripts/fetch-docs-wasm.ts` that the workflow calls
to author the pin — one definition, so the two cannot drift. A released engine
built from different source than the commit being deployed therefore fails the
check rather than shipping.

**None of this can fail the build.** A missing pin, a pin describing other
source, a download that does not arrive, or bytes that do not match their
sha256: each prints one line explaining itself and exits 0. `docs/build.mjs`
detects the engine by whether the artifact is on disk, so without it the
playground page renders and reports that the demo engine is unavailable, exactly
as it did before this mechanism existed. Everything else on the site is
identical. Getting a stale engine, or a wrong one, would be worse than getting
none — so the degradation is deliberate, and the failure mode is a site that
builds.

You can exercise the fetch by hand from the repo root with
`pnpm run docs:wasm:fetch`; contributors with a Rust toolchain can keep building
the engine locally with `pnpm run docs:wasm` instead.

## Notes

- The GitHub-Actions deploy path in `.github/workflows/site-artifact.yml`
  (`deploy-tsrx-dev` job, see `docs/releasing/site-oxc-tsrx-dev.md`) is an
  alternative that ships the WASM artifact too; either can own the domain —
  pick one to avoid competing deploys.
