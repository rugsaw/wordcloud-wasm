# Contributing to wordcloud-wasm

Thanks for your interest in improving wordcloud-wasm! This is a Rust + WebAssembly
layout engine with a thin JS/TS wrapper, and contributions of all kinds — code,
docs, examples, benchmarks, bug reports — are welcome.

## Getting set up

You'll need the Rust toolchain plus the WASM tooling:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack   # or see https://rustwasm.github.io/wasm-pack/
```

Build the package and run the test suite:

```sh
cd crate
wasm-pack build --target web --out-dir ../pkg
cargo test                       # unit + integration tests
cargo test --features parallel   # also exercises the parallel rasterization path
```

To preview the showcase page or examples locally, serve the repo root over HTTP
(browsers block WASM/ES modules over `file://`):

```sh
python -m http.server   # then open http://localhost:8000/
```

See the [README](./README.md) for the full project layout, build variants
(SIMD, multi-threaded), and architecture notes.

## Making changes

- Keep changes focused — one logical change per pull request where possible.
- Match the style of the surrounding code (Rust and JS/TS both).
- Run `cargo fmt` and `cargo test` before opening a PR.
- If you change behavior, update the relevant docs (`README.md`, doc comments,
  or `BENCHMARKS.md`) in the same PR.
- New layout strategies should implement the `LayoutEngine` trait — see the
  scaffolds in [`crate/src/scaffolds.rs`](./crate/src/scaffolds.rs).

## Commit message style

We'd appreciate it if commit messages followed the
[Conventional Commits](https://www.conventionalcommits.org/) convention. It's
**not enforced** — there's no hook blocking your commits — but a consistent
history makes the project much easier to read and to generate changelogs from.

Format the summary line as:

```
<type>(<optional scope>): <short description>
```

Use the `type` that best matches the kind of change:

| Type       | Use it for                                                        |
|------------|-------------------------------------------------------------------|
| `feat`     | A new feature or capability                                       |
| `fix`      | A bug fix                                                          |
| `docs`     | Documentation only (README, doc comments, this file)              |
| `perf`     | A performance improvement                                         |
| `refactor` | Code change that neither fixes a bug nor adds a feature           |
| `test`     | Adding or adjusting tests                                         |
| `bench`    | Benchmark code or results                                         |
| `build`    | Build system, wasm-pack output, dependencies, packaging           |
| `ci`       | CI / automation configuration                                     |
| `chore`    | Maintenance that doesn't fit the above (tidy-ups, config)         |
| `revert`   | Reverting a previous commit                                       |

The optional **scope** points at the area you touched — for this repo that's
usually `crate`, `js`, `pkg`, `examples`, or `docs`.

**Examples:**

```
feat(crate): add circle-packing layout strategy
fix(js): handle zero-weight items in layout client
perf(crate): scale spiral step size to cut candidate count
docs: clarify GitHub Pages deployment steps
test(crate): cover bitmap OR-write edge cases
build: regenerate pkg/ after wasm-pack bump
```

Guidelines for a good message:

- Keep the summary line short (≤ 72 characters) and in the imperative mood
  ("add", not "added"/"adds").
- Add a body after a blank line when the *why* isn't obvious from the summary.
- Append `!` after the type/scope (e.g. `feat(js)!:`) for a breaking change, and
  explain the break in the body.

## Reporting issues

Open an issue at
<https://github.com/rugsaw/wordcloud-wasm/issues> with steps to reproduce, what
you expected, and what happened. For layout/rendering issues, a minimal input
(the `items` array and options you passed) is especially helpful.

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](./LICENSE) that covers this project.
