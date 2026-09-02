{
  description = "uart_16550 UEFI real-hardware test";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              dosfstools
              mtools
              # The full QEMU: qemu_kvm carries only the host architecture's
              # system emulator, but the aarch64 test needs qemu-system-aarch64.
              qemu
              rustup
              socat
              util-linux
            ];
            env.OVMF = "${pkgs.OVMF.fd}/FV/OVMF.fd";
            env.AAVMF_CODE = "${pkgs.qemu}/share/qemu/edk2-aarch64-code.fd";
            env.AAVMF_VARS = "${pkgs.qemu}/share/qemu/edk2-arm-vars.fd";
          };
        }
      );

      formatter = forAllSystems (
        system: nixpkgs.legacyPackages.${system}.nixfmt-tree
      );
    };
}
