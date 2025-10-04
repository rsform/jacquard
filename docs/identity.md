# Identity Resolution

This module provides helpers for resolving AT Protocol identifiers (handles and DIDs) and fetching DID documents.

Highlights:

- DNS TXT (`_atproto.<handle>`) first when compiled with the `dns` feature, then HTTPS well-known, then Slingshot `resolveHandle` when configured as PLC source.
- DID resolution via did:web well-known or PLC base (PLC Directory or Slingshot), returning a `DidDocResponse` that supports borrowed parsing and validation.
- Validation: convenience helpers validate that the fetched DID document `id` matches the requested DID (default on). On mismatch, a `DocIdMismatch` error includes the fetched document for callers to inspect.
- Slingshot: supports unauthenticated `resolveHandle` and a minimal-document endpoint (`com.bad-example.identity.resolveMiniDoc`).
- Auth-aware fallbacks: PDS `resolveHandle` / `resolveDid` available via helpers that accept an `XrpcClient`.

See `jacquard::identity::resolver` rustdoc for examples.

