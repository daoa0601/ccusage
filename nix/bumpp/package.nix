{ callPackage, lib }:
callPackage ../pnpm-tool.nix { } {
  pname = "bumpp";
  version = "11.1.0";
  toolDir = ./.;
  relPath = "nix/bumpp";
  hash = "sha256-eRnqauUGIkd69Txttx5hhd76O2YfwklPr2p54l5qS0c=";
  meta = {
    description = "Interactive CLI that bumps version numbers and tags releases";
    homepage = "https://github.com/antfu-collective/bumpp";
    license = lib.licenses.mit;
  };
}
