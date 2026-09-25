{
  config,
  lib,
  pkgs,
  defaultPackage ? pkgs.oxideterm,
  ...
}:

let
  cfg = config.programs.oxideterm;
in
{
  options.programs.oxideterm = {
    enable = lib.mkEnableOption "OxideTerm";
    package = lib.mkOption {
      type = lib.types.package;
      default = defaultPackage;
      description = "The OxideTerm package to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
