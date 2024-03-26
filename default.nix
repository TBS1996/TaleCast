{ pkgs ? import <nixpkgs> {}, fetchFromGitHub ? pkgs.fetchFromGitHub }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "cringecast";
  version = "0.1.1";

  src = fetchFromGitHub {
    owner = "tbs1996";
    repo = pname;
    rev = "ce1bfc8cfb5b63f4acfbcf801fc363fd39dbc650"; 
    sha256 = "vG1f+gJMaRaLEyNrjJFbztOGFaHgvjXN4NJ0Izh8eD0=";
  };

  cargoSha256 = "kdgjp1soGEreMiR+f1Wcc27vCQCuKbmqAojKj6s7qxA=";


  buildInputs = [ pkgs.openssl pkgs.pkg-config ];

  meta = with pkgs.lib; {
    description = "simple cli podcast manager";
    homepage = "https://github.com/tbs1996/cringecast";
    license = licenses.mit;
    maintainers = with maintainers; [ maintainers.tbs1996 ];
  };
}

