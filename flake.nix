{
  description = "ump-dash - Terminal dashboard for managing UMP React Native worktrees, Metro, and git";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
        };
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          pname = "ump-dash";
          inherit version;
          nativeBuildInputs = with pkgs; [ pkg-config ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        umpdash = craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; });
      in
      {
        packages.default = umpdash;

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            cargo-llvm-cov
            rustfmt
            clippy
            jq
          ];
        };
      }
    ) // {
      overlays.default = final: prev: {
        umpdash = self.packages.${final.stdenv.hostPlatform.system}.default;
      };
    };
}
