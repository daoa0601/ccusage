#!/usr/bin/env nix
#! nix shell --inputs-from ../../.. nixpkgs#nushell --command nu
const system_dylib_prefixes = ['/usr/lib/', '/System/Library/']
def main [] {
    let repo_root = (
        $env.CURRENT_FILE | path dirname | path join ../../.. | path expand
    )
    let target_platform = (node_platform)
    let target_arch = (node_arch)
    let binary_name = if $target_platform == 'win32' { 'ccusage.exe' } else { 'ccusage' }
    let native_package_root = (matching_native_package_root $repo_root $target_platform $target_arch)
    let native_binary = if $native_package_root == null {
        null
    } else {
        $native_package_root | path join bin $binary_name
    }
    let version = (expected_version $repo_root)
    if ((native_package_includes_binary $native_package_root $binary_name) and (has_expected_version $native_binary $version)) {
        if not (is_portable_binary $target_platform $native_binary) {
            error make {
                msg: $"($native_binary) depends on dynamic libraries that do not exist on end-user machines; rebuild it \(Linux packages must be static, macOS packages may only link system dylibs)"
            }
        }
        exit 0
    }
    let built_binary = if ($target_platform == 'linux') or ($target_platform == 'darwin') {
        build_nix_binary $repo_root $target_platform $target_arch $binary_name
    } else {
        build_cargo_binary $repo_root $binary_name
    }
    if not (has_expected_version $built_binary $version) {
        error make {
            msg: $"($built_binary) did not report version ($version) after native build"
        }
    }
    if not (is_portable_binary $target_platform $built_binary) {
        error make {
            msg: $"($built_binary) depends on dynamic libraries that do not exist on end-user machines; rebuild it \(Linux packages must be static, macOS packages may only link system dylibs)"
        }
    }
}
def node_platform [] { match $nu.os-info.name {
    'macos' => 'darwin'
    'linux' => 'linux'
    'windows' => 'win32'
    $other => $other
} }
def node_arch [] { match $nu.os-info.arch {
    'aarch64' => 'arm64'
    'x86_64' => 'x64'
    $other => $other
} }
def matching_native_package_root [repo_root: path, target_platform: string, target_arch: string] {
    let candidates = (glob ($repo_root | path join packages 'ccusage-*' package.json) | each {|package_json_path|
			let package_json = (open $package_json_path)
			let os = $package_json.os?
			let cpu = $package_json.cpu?
			if (list_contains $os $target_platform) and (list_contains $cpu $target_arch) {
				$package_json_path | path dirname
			} else {
				null
			}
		} | where {|package_root| $package_root != null })
    $candidates | get --optional 0
}
def list_contains [value, needle: string] { (($value | describe) =~ '^list') and ($value | any {|item| $item == $needle }) }
def nix_build_attr [target_platform: string, target_arch: string] {
    if $target_platform == 'linux' {
        'ccusage-static'
    } else if $target_platform == 'darwin' and $target_arch == 'x64' {
        'ccusage-darwin-x64'
    } else if $target_platform == 'darwin' and $target_arch == 'arm64' {
        'ccusage'
    } else {
        null
    }
}
def build_nix_binary [
    repo_root: path
    target_platform: string
    target_arch: string
    binary_name: string
] {
    let attr = (nix_build_attr $target_platform $target_arch)
    if $attr == null {
        error make {
            msg: $"No Nix package is configured for ($target_platform)-($target_arch)"
        }
    }
    let flake_ref = $"($repo_root)#($attr)"
    let result = (^nix build $flake_ref '--no-link' '--print-out-paths' '--print-build-logs' | complete)
    if $result.exit_code != 0 {
        error make {
            msg: $"nix build ($flake_ref) failed\n($result.stderr)"
        }
    }
    let out_lines = ($result.stdout | lines | where {|line| $line | is-not-empty })
    if ($out_lines | is-empty) {
        error make {
            msg: $"nix build ($flake_ref) did not print an output path"
        }
    }
    let out_path = ($out_lines | last)
    $out_path | path join bin $binary_name
}
def build_cargo_binary [repo_root: path, executable_name: string] {
    ^cargo build --manifest-path ($repo_root | path join rust Cargo.toml) --release --bin ccusage
    $repo_root | path join rust target release $executable_name
}
def expected_version [repo_root: path] {
    let package_json = (open ($repo_root | path join apps ccusage package.json))
    if (($package_json.version? | describe) != 'string') {
        error make {msg: 'apps/ccusage/package.json version is not configured'}
    }
    $package_json.version
}
def native_package_includes_binary [package_root, binary_name: string] {
    if $package_root == null {
        false
    } else {
        let package_json_path = ($package_root | path join package.json)
        if not ($package_json_path | path exists) {
            false
        } else {
            let package_json = (open $package_json_path)
            let files = $package_json.files?
            (($files | describe) =~ '^list') and ($files | any {|file| $file == $"bin/($binary_name)" })
        }
    }
}
def is_portable_binary [target_platform: string, binary] {
    if $binary == null {
        false
    } else if $target_platform == 'linux' {
        let result = (^ldd $binary | complete)
        let output = $"($result.stdout)($result.stderr)"
        $output =~ '(?i)not a dynamic executable|statically linked'
    } else if $target_platform == 'darwin' {
        let result = (^otool -L $binary | complete)
        if $result.exit_code != 0 {
            false
        } else {
            let dylibs = ($result.stdout | lines | skip 1 | each {|line| $line | str trim } | where {|line| ($line | is-not-empty) } | each {|line| $line | split row --regex '\s+' | get --optional 0 } | where {|dylib| $dylib != null and ($dylib | is-not-empty) })
            $dylibs | all {|dylib|
				$system_dylib_prefixes | any {|prefix| $dylib | str starts-with $prefix }
			}
        }
    } else {
        true
    }
}
def has_expected_version [binary, version: string] {
    if $binary == null or not ($binary | path exists) {
        false
    } else {
        let result = (run-external $binary '--version' | complete)
        if $result.exit_code != 0 {
            false
        } else {
            let actual_version = ($result.stdout | str trim | split row --regex '\s+' | last)
            $actual_version == $version
        }
    }
}
