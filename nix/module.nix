{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.oxideterm;
in
{
  options.programs.oxideterm = {
    enable = lib.mkEnableOption "OxideTerm";
    package = lib.mkPackageOption pkgs "oxideterm" { };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
