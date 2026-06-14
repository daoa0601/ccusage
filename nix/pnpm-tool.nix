# Shared builder for npm CLI tools packaged with pnpm.
#
# Each tool lives in its own directory (e.g. `nix/publint/`) holding a
# `package.json`, a `pnpm-lock.yaml`, and a thin `package.nix` that calls this
# builder with the tool's metadata and the `fetchPnpmDeps` hash.
{
  cacert,
  coreutils,
  curl,
  fetchPnpmDeps,
  gnugrep,
  gnused,
  jq,
  lib,
  makeWrapper,
  nix,
  nodejs,
  pnpmConfigHook,
  pnpm_11,
  stdenvNoCC,
  writeShellApplication,
}:
{
  pname,
  version,
  # FOD hash of the fetched pnpm store for this tool's lockfile.
  hash,
  # Directory containing the tool's `package.json` and `pnpm-lock.yaml`.
  toolDir,
  # Repository-relative path to `toolDir`, used by the update script to rewrite
  # the working-tree files (e.g. "nix/bumpp").
  relPath,
  # Executable to wrap and expose on PATH; defaults to `pname`.
  mainProgram ? pname,
  meta ? { },
}:
let
  pnpm = pnpm_11.override { inherit nodejs; };
  # Build the source from just the manifest and lockfile so the derivation is
  # independent of edits to the sibling `package.nix`.
  src = lib.fileset.toSource {
    root = toolDir;
    fileset = lib.fileset.unions [
      (toolDir + "/package.json")
      (toolDir + "/pnpm-lock.yaml")
    ];
  };

  # Bump the tool to the latest npm release: rewrite the version, regenerate the
  # lockfile, and recompute the fixed-output pnpm deps hash. Exposed as a flake
  # app; run via `nix run .#update-<tool>` or `just update-pnpm-tools`.
  updateScript = writeShellApplication {
    name = "update-${pname}";
    runtimeInputs = [
      cacert
      coreutils
      curl
      gnugrep
      gnused
      jq
      nix
      nodejs
      pnpm
    ];
    text = ''
      dir=${lib.escapeShellArg relPath}
      name=${lib.escapeShellArg pname}

      # Resolve the latest published version from the npm registry.
      latest=$(curl -fsSL "https://registry.npmjs.org/$name/latest" | jq -r '.version')
      echo "Updating $name -> $latest" >&2

      # Bump the declared dependency and the package version attribute.
      tmp=$(mktemp)
      jq --arg name "$name" --arg v "$latest" '.devDependencies[$name] = $v' "$dir/package.json" >"$tmp"
      mv "$tmp" "$dir/package.json"
      sed -i "s|version = \"[^\"]*\";|version = \"$latest\";|" "$dir/package.nix"

      # Regenerate the lockfile for the new version.
      ( cd "$dir" && pnpm install --lockfile-only --ignore-workspace )

      # Recompute the FOD hash: write a placeholder, read the mismatch the build
      # reports, then write the real hash back and verify it builds.
      fake="sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
      sed -i "s|hash = \"sha256-[^\"]*\";|hash = \"$fake\";|" "$dir/package.nix"
      # The placeholder build is expected to fail with a hash mismatch; capture
      # the reported hash without letting the non-zero exit abort the script.
      got=$(nix build ".#$name" --no-link 2>&1 \
        | sed -n 's/.*got:[[:space:]]*\(sha256-[A-Za-z0-9+/=]*\).*/\1/p' | tail -n1 || true)
      if [ -z "$got" ]; then
        echo "Failed to determine new hash for $name" >&2
        exit 1
      fi
      sed -i "s|hash = \"$fake\";|hash = \"$got\";|" "$dir/package.nix"
      nix build ".#$name" --no-link
      echo "Updated $name to $latest" >&2
    '';
  };
in
stdenvNoCC.mkDerivation (finalAttrs: {
  inherit pname version src;

  nativeBuildInputs = [
    makeWrapper
    nodejs
    pnpm
    pnpmConfigHook
  ];

  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs)
      pname
      version
      src
      ;
    inherit pnpm hash;
    fetcherVersion = 3;
  };

  dontBuild = true;

  # `updateProgram` is the bare executable path the flake exposes as a
  # `nix run`-able app (see `just update-pnpm-tools`). It edits the live working
  # tree, so it must run against the real checkout rather than a flake snapshot;
  # `nix run .#update-<tool>` does exactly that.
  passthru.updateProgram = lib.getExe updateScript;

  installPhase = ''
    runHook preInstall

    toolRoot="$out/lib/${pname}"
    mkdir -p "$toolRoot" "$out/bin"
    cp -R node_modules "$toolRoot/node_modules"
    makeWrapper "$toolRoot/node_modules/.bin/${mainProgram}" "$out/bin/${mainProgram}" \
      --prefix PATH : ${lib.makeBinPath [ nodejs ]}

    runHook postInstall
  '';

  meta = {
    inherit mainProgram;
    platforms = lib.platforms.all;
  }
  // meta;
})
