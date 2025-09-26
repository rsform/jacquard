{inputs, ...}: {
  imports = [
    inputs.rust-flake.flakeModules.default
    inputs.rust-flake.flakeModules.nixpkgs
    # inputs.process-compose-flake.flakeModule
    # inputs.cargo-doc-live.flakeModule
  ];
  perSystem = {
    config,
    self',
    pkgs,
    lib,
    ...
  }: let
    inherit (pkgs.stdenv) isDarwin;
    inherit (pkgs.darwin) apple_sdk;

    # Common configuration for all crates
    globalCrateConfig = {
      crane.clippy.enable = false;
    };

    # Common build inputs for all crates
    commonBuildInputs = lib.optionals isDarwin (
      with apple_sdk.frameworks; [
        IOKit
        Security
        SystemConfiguration
      ]
    );
  in {
    rust-project = {
      # Source filtering to avoid unnecessary rebuilds
      src = lib.cleanSourceWith {
        src = inputs.self;
        filter = config.rust-project.crane-lib.filterCargoSources;
      };
      crates = {
        "jacquard" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };

        "jacquard-common" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-common;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };
      };
    };
    packages.default = self'.packages.jacquard;
  };
}
