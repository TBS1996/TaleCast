{ pkgs ? import <nixpkgs> {}, fetchFromGitHub ? pkgs.fetchFromGitHub }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "cringecast";
  version = "0.1.2";

  src = fetchFromGitHub {
    owner = "tbs1996";
    repo = pname;
    rev = "3429b7ab2e51f6fc4f5a4b17b55a2bbb67a7039d"; 
    sha256 = "nG6znVWn3lc2DttiinVeH82GOZVpBj/F7qV9/cum/Pk=";
  };

  cargoSha256 = "LRjbHn1h2Ir+cGUV+K4gU9gyhoL94VYOODN5dcUpeYE=";


  buildInputs = [ pkgs.openssl pkgs.pkg-config ];

  meta = with pkgs.lib; {
    description = "simple cli podcast manager";
    homepage = "https://github.com/tbs1996/cringecast";
    license = licenses.mit;
    maintainers = with maintainers; [ maintainers.tbs1996 ];
  };
}

