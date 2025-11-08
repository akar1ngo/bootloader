{
  description = "Build a cargo project with a custom toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (
          p:
          p.rust-bin.selectLatestNightlyWith (
            toolchain:
            toolchain.default.override {
              extensions = [
                "clippy"
                "rust-analyzer"
                "rust-docs"
                "rust-src"
              ];
              targets = [
                "x86_64-unknown-uefi"
              ];
            }
          )
        );

        bootloader = craneLib.buildPackage {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;

          dummyrs = pkgs.writeText "dummy.rs" ''
            #![allow(unused)]

            #![cfg_attr(
              any(target_os = "none", target_os = "uefi"),
              no_std,
              no_main,
            )]

            #[cfg_attr(any(target_os = "none", target_os = "uefi"), panic_handler)]
            fn panic(_info: &::core::panic::PanicInfo<'_>) -> ! {
                loop {}
            }

            #[cfg_attr(any(target_os = "none", target_os = "uefi"), unsafe(export_name = "efi_main"))]
            pub fn main() {}
          '';

          doCheck = false;
        };

        espDir =
          pkgs.runCommand "esp-dir"
            {
              nativeBuildInputs = [ pkgs.coreutils ];
            }
            ''
              install -Dm444 -t $out/efi/boot ${bootloader}/bin/bootloader.efi
              mv $out/efi/boot/bootloader.efi $out/efi/boot/bootx64.efi
            '';
      in
      {
        checks = {
          inherit bootloader;
        };

        packages.default = bootloader;

        apps.default = flake-utils.lib.mkApp {
          drv = pkgs.writeShellScriptBin "qemu-run" ''
            set -eu
            workdir=$(mktemp -d)
            trap 'rm -rf "$workdir"' EXIT

            cp -r ${espDir}/* "$workdir/"
            chmod -R u+w "$workdir"

            ${pkgs.qemu}/bin/qemu-system-x86_64 \
              -drive if=pflash,format=raw,readonly=on,file="${pkgs.pkgsCross.x86_64-embedded.OVMF.fd}/FV/OVMF_CODE.fd" \
              -drive if=pflash,format=raw,readonly=on,file="${pkgs.pkgsCross.x86_64-embedded.OVMF.fd}/FV/OVMF_VARS.fd" \
              -drive format=raw,file=fat:rw:$workdir
          '';
        };

        devShells.default = craneLib.devShell {
          packages = [
            pkgs.qemu
          ];

          OVMF_CODE_PATH = "${pkgs.pkgsCross.x86_64-embedded.OVMF.fd}/FV/OVMF_CODE.fd";
          OVMF_VARS_PATH = "${pkgs.pkgsCross.x86_64-embedded.OVMF.fd}/FV/OVMF_VARS.fd";
        };
      }
    );
}
