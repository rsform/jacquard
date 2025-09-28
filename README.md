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



### String types
Something of a note to self. Developing a pattern with the string types (may macro-ify at some point). Each needs:
- new(): constructing from a string slice with the right lifetime that borrows
- new_owned(): constructing from an impl AsRef<str>, taking ownership
- new_static(): construction from a &'static str, using SmolStr's/CowStr's new_static() constructor to not allocate
- raw(): same as new() but panics instead of erroring
- unchecked(): same as new() but doesn't validate. marked unsafe.
- as_str(): does what it says on the tin
#### Traits:
- Serialize + Deserialize (custom impl for latter, sometimes for former)
- FromStr
- Display
- Debug, PartialEq, Eq, Hash, Clone
- From<T> for String, CowStr, SmolStr,
- From<String>, From<CowStr>, From<SmolStr>, or TryFrom if likely enough to fail in practice to make panics common
- AsRef<str>
- Deref with Target = str (usually)

Use `#[repr(transparent)]` as much as possible. Main exception is at-uri type and components.
Use SmolStr directly as the inner type if most or all of the instances will be under 24 bytes, save lifetime headaches.
Use CowStr for longer to allow for borrowing from input.

TODO: impl IntoStatic trait to take ownership of string types
