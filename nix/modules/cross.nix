{inputs, ...}: {
  imports = [inputs.rust-flake.flakeModules.nixpkgs];

  perSystem = {pkgs, lib, config, system, ...}: let
    # Get the filtered source from rust-project
    src = config.rust-project.src;

    # Import nixpkgs with rust-overlay for getting rust toolchains (not cross)
    pkgs-rust = import inputs.nixpkgs {
      inherit system;
      overlays = [(import inputs.rust-overlay)];
    };

    # Helper to create a cross-compiled package
    mkCrossPackage = {
      crossSystem,
      rustTarget,
      extraArgs ? {}
    }: let
      # Import nixpkgs with cross-compilation configured (no overlays)
      pkgs-cross = import inputs.nixpkgs {
        inherit crossSystem;
        localSystem = system;
      };

      # Get rust toolchain from host system with the cross target added
      rustToolchain = pkgs-rust.rust-bin.stable.latest.default.override {
        targets = [rustTarget];
      };

      # Set up crane with the rust toolchain
      craneLib = (inputs.crane.mkLib pkgs-cross).overrideToolchain rustToolchain;

      # Common crane args
      commonArgs = {
        inherit src;
        pname = "jacquard-lexicon";
        strictDeps = true;
        doCheck = false;  # Tests require lexicon corpus files

        # Native build inputs (tools that run during build)
        nativeBuildInputs = with pkgs; [
          installShellFiles
        ];

        postInstall = ''
          # Install man pages and completions from build script output
          for outdir in target/${rustTarget}/release/build/jacquard-lexicon-*/out; do
            if [ -d "$outdir/man" ]; then
              installManPage $outdir/man/*.1
            fi
            if [ -d "$outdir/completions" ]; then
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
      } // extraArgs;
    in
      craneLib.buildPackage commonArgs;
  in {
    packages = {
      # Linux targets
      jacquard-lexicon-x86_64-linux = mkCrossPackage {
        crossSystem = {
          config = "x86_64-unknown-linux-gnu";
        };
        rustTarget = "x86_64-unknown-linux-gnu";
      };

      jacquard-lexicon-aarch64-linux = mkCrossPackage {
        crossSystem = {
          config = "aarch64-unknown-linux-gnu";
        };
        rustTarget = "aarch64-unknown-linux-gnu";
      };

      # macOS targets
      jacquard-lexicon-x86_64-darwin = mkCrossPackage {
        crossSystem = {
          config = "x86_64-apple-darwin";
        };
        rustTarget = "x86_64-apple-darwin";
      };

      jacquard-lexicon-aarch64-darwin = mkCrossPackage {
        crossSystem = {
          config = "aarch64-apple-darwin";
        };
        rustTarget = "aarch64-apple-darwin";
      };

      # Windows targets
      jacquard-lexicon-x86_64-windows = mkCrossPackage {
        crossSystem = {
          config = "x86_64-w64-mingw32";
          libc = "msvcrt";
        };
        rustTarget = "x86_64-pc-windows-gnu";
      };

      # TODO: aarch64-windows cross-compilation broken in nixpkgs
      # Issue: mingw-w64-pthreads build fails with missing winver.h header
      # The aarch64-w64-mingw32 toolchain setup in nixpkgs is incomplete
      # Workaround: build on actual Windows with MSVC or wait for nixpkgs fix
      # jacquard-lexicon-aarch64-windows = mkCrossPackage {
      #   crossSystem = {
      #     config = "aarch64-w64-mingw32";
      #     useLLVM = true;
      #   };
      #   rustTarget = "aarch64-pc-windows-gnullvm";
      # };
    };
  };
}
