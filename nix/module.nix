# Shared options for the NixOS and home-manager modules.
{ self }:
{
  nixos =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      cfg = config.programs.zl;
    in
    {
      options.programs.zl = {
        enable = lib.mkEnableOption "zl, a small Wayland app launcher";
        package = lib.mkOption {
          type = lib.types.package;
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
          defaultText = lib.literalExpression "zl.packages.\${pkgs.system}.default";
          description = "The zl package to install.";
        };
        systemd.enable = lib.mkEnableOption "the zl daemon as a systemd user service of graphical sessions";
      };

      config = lib.mkIf cfg.enable {
        environment.systemPackages = [ cfg.package ];
        systemd.user.services.zl = lib.mkIf cfg.systemd.enable {
          description = "zl Wayland launcher";
          partOf = [ "graphical-session.target" ];
          wantedBy = [ "graphical-session.target" ];
          serviceConfig = {
            ExecStart = lib.getExe cfg.package;
            Restart = "on-failure";
            RestartSec = 2;
          };
        };
      };
    };

  homeManager =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      cfg = config.programs.zl;
      toml = pkgs.formats.toml { };
    in
    {
      options.programs.zl = {
        enable = lib.mkEnableOption "zl, a small Wayland app launcher";
        package = lib.mkOption {
          type = lib.types.package;
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
          defaultText = lib.literalExpression "zl.packages.\${pkgs.system}.default";
          description = "The zl package to install.";
        };
        settings = lib.mkOption {
          type = toml.type;
          default = { };
          example = lib.literalExpression ''
            {
              theme = "nord";
              max_clipboard_history = 200;
              font.font_size = 15.0;
            }
          '';
          description = ''
            Written to {file}`$XDG_CONFIG_HOME/zl/config.toml`. Leave empty to manage the file yourself.
            Restart the daemon after changes.
          '';
        };
        themes = lib.mkOption {
          type = lib.types.attrsOf toml.type;
          default = { };
          example = lib.literalExpression ''{ glass = { window_background = "#19375a80"; }; }'';
          description = "User themes, written to {file}`$XDG_CONFIG_HOME/zl/themes/NAME.toml`.";
        };
        systemd.enable = lib.mkEnableOption "the zl daemon as a systemd user service";
      };

      config = lib.mkIf cfg.enable {
        home.packages = [ cfg.package ];
        xdg.configFile = lib.mkMerge [
          (lib.mkIf (cfg.settings != { }) { "zl/config.toml".source = toml.generate "zl-config.toml" cfg.settings; })
          (lib.mapAttrs' (
            name: value: lib.nameValuePair "zl/themes/${name}.toml" { source = toml.generate "zl-theme-${name}.toml" value; }
          ) cfg.themes)
        ];
        systemd.user.services.zl = lib.mkIf cfg.systemd.enable {
          Unit = {
            Description = "zl Wayland launcher";
            PartOf = [ "graphical-session.target" ];
            After = [ "graphical-session.target" ];
          };
          Service = {
            ExecStart = lib.getExe cfg.package;
            Restart = "on-failure";
            RestartSec = 2;
          };
          Install.WantedBy = [ "graphical-session.target" ];
        };
      };
    };
}
