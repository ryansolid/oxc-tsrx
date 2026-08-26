# `@tsrx/oxc-core-compat`

A small `@tsrx/core`-compatible parser facade for Markless and similar consumers. It delegates
TSRX parsing to `@tsrx/oxc/parser`, the parser export of the one public `@tsrx/oxc` package, and
provides the event-name helpers used by Markless.

This package does not provide editor recovery. `loose` and `collect` collect diagnostics only
when the native parser can still return a program.
