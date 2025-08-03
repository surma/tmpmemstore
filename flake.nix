{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      flake-utils,
      nixpkgs,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (system: rec {
      packages = {
        tmpmemstore = nixpkgs.legacyPackages.${system}.callPackage ./. { };
        default = packages.tmpmemstore;
      };
      apps = {
        default = apps.tmpmemstore;
        tmpmemstore = {
          type = "app";
          program = "${packages.tmpmemstore}/bin/tmpmemstore";
        };
      };
    });
}
