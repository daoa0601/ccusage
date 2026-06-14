{ callPackage, lib }:
callPackage ../pnpm-tool.nix { } {
  pname = "changelogithub";
  version = "14.0.0";
  toolDir = ./.;
  relPath = "nix/changelogithub";
  hash = "sha256-MoRS1PvGDZJgX6tZ7EyYh/9HrbwrMPS1418bWNbnkH0=";
  meta = {
    description = "Generate changelog for GitHub releases from conventional commits";
    homepage = "https://github.com/antfu/changelogithub";
    license = lib.licenses.mit;
  };
}
