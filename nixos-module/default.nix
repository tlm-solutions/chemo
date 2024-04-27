{ pkgs, config, lib, ... }:
let
  cfg = config.TLMS.chemo;
in
{
  options.TLMS.chemo = with lib; {
    enable = mkOption {
      type = types.bool;
      default = false;
      description = ''Wether to enable chemo service'';
    };
    host = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = ''
        To which IP chemo should bind its grpc server.
      '';
    };
    port = mkOption {
      type = types.port;
      default = 8080;
      description = ''
        To which port should chemo bind its grpc_server.
      '';
    };
    database = {
      host = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = ''
          Database host
        '';
      };
      port = mkOption {
        type = types.port;
        default = 5354;
        description = ''
          Database port
        '';
      };
      user = mkOption {
        type = types.str;
        default = "chemo";
        description = ''
          user for postgres
        '';
      };
      database = mkOption {
        type = types.str;
        default = "tlms";
        description = ''
          postgres database to use
        '';
      };
      passwordFile = mkOption {
        type = types.either types.path types.string;
        default = "";
        description = ''password file from which the postgres password can be read'';
      };
    };
    user = mkOption {
      type = types.str;
      default = "chemo";
      description = ''systemd user'';
    };
    group = mkOption {
      type = types.str;
      default = "chemo";
      description = ''group of systemd user'';
    };
    log_level = mkOption {
      type = types.str;
      default = "info";
      description = ''log level of the application'';
    };
    GRPC = mkOption {
      type = types.listOf
        (types.submodule {
          options.schema = mkOption {
            type = types.enum [ "http" "https" ];
            default = "http";
            description = ''
              schema to connect to GRPC
            '';
          };
          options.name = mkOption {
            type = types.str;
            default = "";
            description = ''
              GRPC name
            '';
          };
          options.host = mkOption {
            type = types.str;
            default = "127.0.0.1";
            description = ''
              GRPC: schema://hostname
            '';
          };
          options.port = mkOption {
            type = types.port;
            default = 50051;
            description = ''
              GRPC port
            '';
          };
        });
        default = [ ];
        description = ''list of grpc endpoint where chemo should send data to'';
    };
  };

  config = lib.mkIf cfg.enable {
    systemd = {
      services = {
        "chemo" = {
          enable = true;
          wantedBy = [ "multi-user.target" ];

          script = ''
            exec ${pkgs.chemo}/bin/chemo&
          '';

          environment = {
            "RUST_LOG" = "${cfg.log_level}";
            "RUST_BACKTRACE" = if (cfg.log_level == "info") then "0" else "1";
	        "CHEMO_HOST" = "${cfg.host}:${toString cfg.port}";
            "CHEMO_POSTGRES_PASSWORD_PATH" = "${cfg.database.passwordFile}";
            "CHEMO_POSTGRES_HOST" = "${cfg.database.host}";
            "CHEMO_POSTGRES_PORT" = "${toString cfg.database.port}";
            "CHEMO_POSTGRES_USER" = "${toString cfg.database.user}";
            "CHEMO_POSTGRES_DATABASE" = "${toString cfg.database.database}";
          } // (lib.foldl
            (x: y:
              lib.mergeAttrs x { "CHEMO_GRPC_HOST_${y.name}" = "${y.schema}://${y.host}:${toString y.port}"; })
            { }
            cfg.GRPC);

          serviceConfig = {
            Type = "forking";
            User = cfg.user;
            Restart = "always";
          };
        };
      };
    };

    # user accounts for systemd units
    users.users."${cfg.user}" = {
      name = "${cfg.user}";
      description = "This guy runs chemo";
      isNormalUser = false;
      isSystemUser = true;
      group = cfg.group;
      extraGroups = [ ];
    };
    users.groups."${cfg.group}" = {};
  };
}
