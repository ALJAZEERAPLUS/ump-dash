{ config, lib, pkgs, ... }:
with lib;
let
  cfg = config.programs.ump-dash;
  tomlFormat = pkgs.formats.toml { };
in {
  meta.maintainers = [ ];

  options.programs.ump-dash = {
    enable = mkEnableOption "ump-dash — terminal dashboard for UMP worktrees";

    package = mkPackageOption pkgs "umpdash" { };

    settings = mkOption {
      type = tomlFormat.type;
      default = { };
      example = lib.literalExpression ''
        {
          repo_root = "~/code/my-rn-app";
          jira_base_url = "https://your-org.atlassian.net";
          jira_email = "you@example.com";
          jira_token = "your-api-token-here";
        }
      '';
      description = ''
        Configuration written to
        <filename>XDG_CONFIG_HOME/ump-dash/config.toml</filename>.
        The file is created with 0600 permissions because it contains
        credentials.  See
        <link xlink:href="https://github.com/ALJAZEERAPLUS/ump-dash/blob/main/config.example.toml"/>
        for all available options.
      '';
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."ump-dash/config.toml" = {
      source = tomlFormat.generate "ump-dash-config.toml" cfg.settings;
      force = true;
      onChange = lib.mkDefault "";
    };
  };
}
