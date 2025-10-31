{inputs, ...}: {
  perSystem = {
    config,
    self',
    pkgs,
    lib,
    ...
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
        zip
      ];
    };
    apps = {
      default.program = "${self'.packages.jacquard-lexgen}/bin/lex-fetch";
    };
  };
}
