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
              qemu_kvm
              rustup
              socat
              util-linux
            ];
            env.OVMF = "${pkgs.OVMF.fd}/FV/OVMF.fd";
          };
        }
      );

      formatter = forAllSystems (
        system: nixpkgs.legacyPackages.${system}.nixfmt-tree
      );
    };
}
