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
              doCheck = false;  # Tests require lexicon corpus files not available in nix build
              postInstall = ''
                # Install man pages
                if [ -d "$OUT_DIR/man" ]; then
                  install -Dm644 $OUT_DIR/man/*.1 -t $out/share/man/man1/
                fi

                # Install shell completions
                if [ -d "$OUT_DIR/completions" ]; then
                  install -Dm644 $OUT_DIR/completions/lex-fetch.bash $out/share/bash-completion/completions/lex-fetch
                  install -Dm644 $OUT_DIR/completions/lex-fetch.fish $out/share/fish/vendor_completions.d/lex-fetch.fish
                  install -Dm644 $OUT_DIR/completions/_lex-fetch $out/share/zsh/site-functions/_lex-fetch
                fi
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
