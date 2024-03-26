{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = [
    pkgs.openssl
    pkgs.pkg-config
    pkgs.rustc
    pkgs.cargo
    pkgs.rustup
    pkgs.rust-analyzer
  ];
}


