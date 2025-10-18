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
    commonBuildInputs = with pkgs;
      [
      ]
      ++ lib.optionals
      isDarwin (
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
        "jacquard-derive" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-derive;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };
        "jacquard-lexicon" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-lexicon;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
              nativeBuildInputs = [pkgs.installShellFiles];
              doCheck = false;  # Tests require lexicon corpus files not available in nix build
              postInstall = ''
                # Install man pages and completions from build script output
                for outdir in target/release/build/jacquard-lexicon-*/out; do
                  if [ -d "$outdir/man" ]; then
                    installManPage $outdir/man/*.1
                  fi
                  if [ -d "$outdir/completions" ]; then
                    # Install completions for both binaries
                    for completion in $outdir/completions/*; do
                      case "$(basename "$completion")" in
                        *.bash) installShellCompletion --bash "$completion" ;;
                        *.fish) installShellCompletion --fish "$completion" ;;
                        _*) installShellCompletion --zsh "$completion" ;;
                      esac
                    done
                  fi
                done

                # Install example lexicons.kdl config
                install -Dm644 ${./../../crates/jacquard-lexicon/lexicons.kdl.example} $out/share/doc/jacquard-lexicon/lexicons.kdl.example
              '';
            };
          };
        };
        "jacquard-api" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-api;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };
        "jacquard-identity" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-identity;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };
        "jacquard-oauth" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-oauth;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };
        "jacquard-axum" = {
          imports = [globalCrateConfig];
          autoWire = ["crate" "clippy"];
          path = ./../../crates/jacquard-axum;
          crane = {
            args = {
              buildInputs = commonBuildInputs;
            };
          };
        };
      };
    };
    packages.default = self'.packages.jacquard-lexicon;
  };
}
