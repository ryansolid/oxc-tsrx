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

## Notes

- The playground's WASM demo engine is an optional prebuilt artifact; the CI
  pipeline includes it, a plain Vercel build omits it (the playground page still
  renders and says the demo engine is unavailable). Everything else is identical.
- The GitHub-Actions deploy path in `.github/workflows/site-artifact.yml`
  (`deploy-tsrx-dev` job, see `docs/releasing/site-oxc-tsrx-dev.md`) is an
  alternative that ships the WASM artifact too; either can own the domain —
  pick one to avoid competing deploys.
