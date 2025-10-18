{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    systems.url = "github:nix-systems/default";
    rust-flake.url = "github:juspay/rust-flake";
    rust-flake.inputs.nixpkgs.follows = "nixpkgs";
    #process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
    #cargo-doc-live.url = "github:srid/cargo-doc-live";

    # For cross-compilation
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";

    git-hooks.url = "github:cachix/git-hooks.nix";
    git-hooks.flake = false;
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      systems = import inputs.systems;

      # See ./nix/modules/*.nix for the modules that are imported here.
      imports = with builtins;
        map
        (fn: ./nix/modules/${fn})
        (attrNames (readDir ./nix/modules));
    };
}
