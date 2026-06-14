{
  inputs,
  ...
}:
let
  root = ./..;
in
{
  perSystem =
    {
      system,
      ...
    }:
    let
      pkgs = import inputs.nixpkgs {
        inherit system;
        overlays = [ inputs.rust-overlay.overlays.default ];
      };
      rustToolchain = pkgs.rust-bin.fromRustupToolchainFile (root + /rust-toolchain.toml);
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;
      ccusage = import ../default.nix {
        inherit
          craneLib
          inputs
          pkgs
          root
          ;
      };
      ccusageProgram = pkgs.lib.getExe' ccusage "ccusage";
      # Regeneration-only output for committed models.dev snapshots;
      # `just gen-models-dev-pricing` builds this and copies them into the source
      # tree. It is not part of the ccusage build, which embeds the committed files.
      models-dev-pricing = pkgs.callPackage ../nix/models-dev-pricing.nix {
        modelsDevSrc = inputs.models-dev;
      };
      publint = pkgs.callPackage ../nix/publint/package.nix { };
      bumpp = pkgs.callPackage ../nix/bumpp/package.nix { };
      changelogithub = pkgs.callPackage ../nix/changelogithub/package.nix { };
    in
    {
      apps = {
        default = {
          type = "app";
          program = ccusageProgram;
        };
        ccusage = {
          type = "app";
          program = ccusageProgram;
        };
        update-publint = {
          type = "app";
          program = publint.updateProgram;
        };
        update-bumpp = {
          type = "app";
          program = bumpp.updateProgram;
        };
        update-changelogithub = {
          type = "app";
          program = changelogithub.updateProgram;
        };
      };

      packages = {
        default = ccusage;
        inherit
          ccusage
          models-dev-pricing
          publint
          bumpp
          changelogithub
          ;
      };
    };
}
