{ pkgs ? import <nixpkgs> {}, fetchFromGitHub ? pkgs.fetchFromGitHub }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "cringecast";
  version = "0.1.0";

  src = fetchFromGitHub {
    owner = "tbs1996";
    repo = pname;
    rev = "117b01c6f6d723d930290aef7d73f24edee19247"; 
    sha256 = "0000000000000000000000000000000000000000000000000000"; 
  };

  cargoSha256 = "0000000000000000000000000000000000000000000000000000"; 

  buildInputs = [ pkgs.openssl pkgs.pkg-config ];

  meta = with pkgs.lib; {
    description = "simple cli podcast manager";
    homepage = "https://github.com/tbs1996/cringecast";
    license = licenses.mit;
    maintainers = with maintainers; [ maintainers.<your-nixpkgs-username> ];
  };
}

