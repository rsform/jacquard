# Jacquard

A suite of Rust crates for the AT Protocol.

## Goals

- Validated, spec-compliant, easy to work with, and performant baseline types (including typed at:// uris)
- Batteries-included, but easily replaceable batteries.
  - Easy to extend with custom lexicons
- lexicon Value type for working with unknown atproto data (dag-cbor or json)
- order of magnitude less boilerplate than some existing crates
  - either the codegen produces code that's easy to work with, or there are good handwritten wrappers
- didDoc type with helper methods for getting handles, multikey, and PDS endpoint
- use as much or as little from the crates as you need

## Development

This repo uses [Flakes](https://nixos.asia/en/flakes) from the get-go.

```bash
# Dev shell
nix develop

# or run via cargo
nix develop -c cargo run

# build
nix build
```

There's also a [`justfile`](https://just.systems/) for Makefile-esque commands to be run inside of the devShell, and you can generally `cargo ...` or `just ...` whatever just fine if you don't want to use Nix and have the prerequisites installed.
