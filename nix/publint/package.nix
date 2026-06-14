{ callPackage, lib }:
callPackage ../pnpm-tool.nix { } {
  pname = "publint";
  version = "0.3.12";
  toolDir = ./.;
  relPath = "nix/publint";
  hash = "sha256-TSLDCjiXj131uLlLNegm6M4cFEgJtjmhNtHDtoHKm6E=";
  meta = {
    description = "Lint packaging errors";
    homepage = "https://publint.dev";
    license = lib.licenses.mit;
  };
}
