{ inputs, ... }: {
  perSystem =
    { config
    , self'
    , pkgs
    , lib
    , ...
    }: {
      devShells.default = pkgs.mkShell {
        name = "jacquard-shell";
        inputsFrom = [
          self'.devShells.rust
          config.pre-commit.devShell # See ./nix/modules/pre-commit.nix
        ];
        packages = with pkgs; [
          just
          nixd # Nix language server
          bacon
          rust-analyzer
          cargo-machete
          cargo-semver-checks
          cargo-binstall
          cargo-dist
          cargo-nextest
          zip
          atproto-goat
        ];
      };
      apps = {
        default.program = "${self'.packages.jacquard-lexgen}/bin/lex-fetch";
        lexgen.program = "${self'.packages.jacquard-lexgen}/bin/lex-fetch";
      };

      # Opt-in shell for the full-stack e2e harness (`nix develop .#e2e -c
      # just e2e`). Inherits the default developer shell and adds only the
      # orchestration tools the lifecycle controller needs.
      devShells.e2e = pkgs.mkShell {
        name = "jacquard-e2e-shell";
        inputsFrom = [ self'.devShells.default ];
        packages = with pkgs; [
          docker-client
          docker-buildx
          docker-compose
          curl
          jq
          openssl
          python3
        ];
      };
    };
}
