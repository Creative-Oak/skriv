# TypeScript packages

Empty for now. The web UI will live here, along with the `@skriv/*` packages
built from the Rust core with `wasm-bindgen`. Those packages are how birk will
embed skriv: as a git or file dependency in birk's pnpm workspace. Tooling
(pnpm 10, Node 22+, TypeScript 7, Biome 2.6, Vitest 5) matches birk's on
purpose.
