{
  pandarAgentPackage,
  pandarAuthPackage,
  pandarHubPackage,
  pandarWebPackage,
}:
{
  config,
  lib,
  ...
}:
let
  cfg = config.services.pandar;
  authCfg = config.services.pandar-auth;
  natsServiceUrl = "nats://127.0.0.1:4222";
  natsUrl = if cfg.hub.nats.mode == "service" then natsServiceUrl else cfg.hub.nats.url;
  authBindParts = builtins.match "(.+):([0-9]+)" authCfg.bind;
  authBind =
    if authBindParts == null then
      {
        host = "";
        port = "";
      }
    else
      {
        host = builtins.elemAt authBindParts 0;
        port = builtins.elemAt authBindParts 1;
      };
in
{
  options.services = {
    pandar = {
      enable = lib.mkEnableOption "Pandar hub and web services";

      hub = {
        enable = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = "Whether to run pandar-hub when Pandar is enabled.";
        };

        package = lib.mkOption {
          type = lib.types.package;
          default = pandarHubPackage;
          description = "pandar-hub package to run.";
        };

        bind = lib.mkOption {
          type = lib.types.str;
          default = "0.0.0.0:8080";
          description = "HTTP bind address for pandar-hub.";
        };

        grpcBind = lib.mkOption {
          type = lib.types.str;
          default = "0.0.0.0:50051";
          description = "gRPC bind address for pandar-hub agent connections.";
        };

        databaseUrl = lib.mkOption {
          type = lib.types.str;
          default = "sqlite:///var/lib/pandar-hub/pandar.db";
          description = "Database URL passed through PANDAR_DATABASE_URL.";
        };

        controlPlane = lib.mkOption {
          type = lib.types.enum [
            "in-process"
            "nats"
          ];
          default = "in-process";
          description = "Hub control plane passed through PANDAR_CONTROL_PLANE.";
        };

        nats = {
          mode = lib.mkOption {
            type = lib.types.enum [
              "external"
              "service"
            ];
            default = "external";
            description = ''
              NATS source for the hub control plane. `external` uses `services.pandar.hub.nats.url`;
              `service` enables the local NixOS NATS service and points the hub at it.
            '';
          };

          url = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "External NATS URL passed through PANDAR_NATS_URL when the hub uses the NATS control plane.";
          };

          subject = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Optional NATS subject passed through PANDAR_NATS_SUBJECT.";
          };
        };

        spoolDir = lib.mkOption {
          type = lib.types.path;
          default = "/var/lib/pandar-hub/spool";
          description = "Artifact spool directory passed through PANDAR_SPOOL_DIR.";
        };

        extraEnvironment = lib.mkOption {
          type = lib.types.attrsOf lib.types.str;
          default = { };
          description = "Extra environment variables for pandar-hub.";
        };
      };

      web = {
        enable = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = "Whether to run pandar-web when Pandar is enabled.";
        };

        package = lib.mkOption {
          type = lib.types.package;
          default = pandarWebPackage;
          description = "pandar-web package to run.";
        };

        port = lib.mkOption {
          type = lib.types.port;
          default = 3000;
          description = "HTTP port for pandar-web.";
        };

        apiUrl = lib.mkOption {
          type = lib.types.str;
          default = "http://127.0.0.1:8080";
          description = "Rust API URL passed through APP_API_URL.";
        };

        baseUrl = lib.mkOption {
          type = lib.types.str;
          default = "http://127.0.0.1:3000";
          description = "Public frontend URL passed through APP_BASE_URL.";
        };

        extraEnvironment = lib.mkOption {
          type = lib.types.attrsOf lib.types.str;
          default = { };
          description = "Extra environment variables for pandar-web.";
        };
      };

      agent = {
        enable = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Whether to run pandar-agent when Pandar is enabled.";
        };

        package = lib.mkOption {
          type = lib.types.package;
          default = pandarAgentPackage;
          description = "pandar-agent package to run.";
        };

        hubGrpcUrl = lib.mkOption {
          type = lib.types.str;
          default = "http://127.0.0.1:50051";
          description = "Hub gRPC URL passed through PANDAR_HUB_GRPC_URL.";
        };

        name = lib.mkOption {
          type = lib.types.str;
          default = "local-agent";
          description = "Agent name passed through PANDAR_AGENT_NAME.";
        };

        agentId = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Agent ID passed through PANDAR_AGENT_ID.";
        };

        tenantId = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Tenant ID passed through PANDAR_TENANT_ID.";
        };

        credential = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Agent credential passed through PANDAR_AGENT_CREDENTIAL.";
        };

        printers = lib.mkOption {
          type = lib.types.str;
          default = "[]";
          description = "Printer endpoint JSON passed through PANDAR_PRINTERS.";
        };

        artifactRoot = lib.mkOption {
          type = lib.types.path;
          default = "/var/lib/pandar-agent/artifacts";
          description = "Artifact root passed through PANDAR_ARTIFACT_ROOT.";
        };

        environmentFile = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
          description = "Optional systemd EnvironmentFile for agent secrets.";
        };

        extraEnvironment = lib.mkOption {
          type = lib.types.attrsOf lib.types.str;
          default = { };
          description = "Extra environment variables for pandar-agent.";
        };
      };
    };

    pandar-auth = {
      enable = lib.mkEnableOption "self-hosted Pandar Better Auth issuer";

      package = lib.mkOption {
        type = lib.types.package;
        default = pandarAuthPackage;
        description = "pandar-auth package to run.";
      };

      bind = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1:3001";
        description = "HTTP bind address for pandar-auth, formatted as host:port.";
      };

      baseURL = lib.mkOption {
        type = lib.types.str;
        default = "http://127.0.0.1:3001";
        description = "Public Better Auth URL passed through PANDAR_AUTH_BASE_URL.";
      };

      trustedOrigins = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ "http://127.0.0.1:3000" ];
        description = "Trusted dashboard origins passed through PANDAR_AUTH_TRUSTED_ORIGINS.";
      };

      dashboardCallbackUrl = lib.mkOption {
        type = lib.types.str;
        default = "http://127.0.0.1:3000/auth/betterauth/callback";
        description = "Dashboard callback URL passed through PANDAR_AUTH_DASHBOARD_CALLBACK_URL.";
      };

      dashboardSignOutUrl = lib.mkOption {
        type = lib.types.str;
        default = "http://127.0.0.1:3000/auth/betterauth/sign-out";
        description = "Dashboard sign-out URL passed through PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL.";
      };

      databaseFile = lib.mkOption {
        type = lib.types.path;
        default = "/var/lib/pandar-auth/auth.db";
        description = "SQLite database file passed through PANDAR_AUTH_DATABASE_FILE.";
      };

      jwtMaxAgeSeconds = lib.mkOption {
        type = lib.types.ints.positive;
        default = 43200;
        description = "Better Auth JWT expiration in seconds passed through PANDAR_AUTH_JWT_MAX_AGE_SECONDS.";
      };

      environmentFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Optional systemd EnvironmentFile for Better Auth secrets such as BETTER_AUTH_SECRET.";
      };

      extraEnvironment = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = { };
        description = "Extra environment variables for pandar-auth.";
      };
    };
  };

  config = lib.mkMerge [
    {
      assertions = [
        {
          assertion = !cfg.enable || !cfg.hub.enable || cfg.hub.controlPlane != "nats" || natsUrl != null;
          message = "services.pandar.hub.nats.url is required when services.pandar.hub.controlPlane is \"nats\" and services.pandar.hub.nats.mode is \"external\".";
        }
        {
          assertion = !authCfg.enable || authBindParts != null;
          message = "services.pandar-auth.bind must be formatted as host:port.";
        }
        {
          assertion =
            !authCfg.enable || authCfg.environmentFile != null || authCfg.extraEnvironment ? BETTER_AUTH_SECRET;
          message = "services.pandar-auth requires BETTER_AUTH_SECRET through services.pandar-auth.environmentFile or services.pandar-auth.extraEnvironment.";
        }
      ];
    }

    (lib.mkIf cfg.enable {

      services.nats.enable = lib.mkIf (
        cfg.hub.enable && cfg.hub.controlPlane == "nats" && cfg.hub.nats.mode == "service"
      ) true;

      systemd.services.pandar-hub = lib.mkIf cfg.hub.enable {
        description = "Pandar hub";
        wantedBy = [ "multi-user.target" ];
        after = [
          "network.target"
        ]
        ++ lib.optional (cfg.hub.controlPlane == "nats" && cfg.hub.nats.mode == "service") "nats.service";
        wants = lib.optional (
          cfg.hub.controlPlane == "nats" && cfg.hub.nats.mode == "service"
        ) "nats.service";

        environment = {
          PANDAR_HUB_BIND = cfg.hub.bind;
          PANDAR_HUB_GRPC_BIND = cfg.hub.grpcBind;
          PANDAR_DATABASE_URL = cfg.hub.databaseUrl;
          PANDAR_CONTROL_PLANE = cfg.hub.controlPlane;
          PANDAR_SPOOL_DIR = toString cfg.hub.spoolDir;
        }
        // lib.optionalAttrs (natsUrl != null) {
          PANDAR_NATS_URL = natsUrl;
        }
        // lib.optionalAttrs (cfg.hub.nats.subject != null) {
          PANDAR_NATS_SUBJECT = cfg.hub.nats.subject;
        }
        // cfg.hub.extraEnvironment;

        serviceConfig = {
          ExecStart = "${cfg.hub.package}/bin/pandar-hub";
          DynamicUser = true;
          StateDirectory = "pandar-hub";
          WorkingDirectory = "/var/lib/pandar-hub";
          Restart = "on-failure";
          RestartSec = "5s";
        };
      };

      systemd.services.pandar-web = lib.mkIf cfg.web.enable {
        description = "Pandar web";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ] ++ lib.optional cfg.hub.enable "pandar-hub.service";
        wants = lib.optional cfg.hub.enable "pandar-hub.service";

        environment = {
          PORT = toString cfg.web.port;
          APP_API_URL = cfg.web.apiUrl;
          APP_BASE_URL = cfg.web.baseUrl;
        }
        // cfg.web.extraEnvironment;

        serviceConfig = {
          ExecStart = "${cfg.web.package}/bin/pandar-web";
          DynamicUser = true;
          Restart = "on-failure";
          RestartSec = "5s";
        };
      };

      systemd.services.pandar-agent = lib.mkIf cfg.agent.enable {
        description = "Pandar agent";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ] ++ lib.optional cfg.hub.enable "pandar-hub.service";
        wants = lib.optional cfg.hub.enable "pandar-hub.service";

        environment = {
          PANDAR_HUB_GRPC_URL = cfg.agent.hubGrpcUrl;
          PANDAR_AGENT_NAME = cfg.agent.name;
          PANDAR_PRINTERS = cfg.agent.printers;
          PANDAR_ARTIFACT_ROOT = toString cfg.agent.artifactRoot;
        }
        // lib.optionalAttrs (cfg.agent.agentId != null) {
          PANDAR_AGENT_ID = cfg.agent.agentId;
        }
        // lib.optionalAttrs (cfg.agent.tenantId != null) {
          PANDAR_TENANT_ID = cfg.agent.tenantId;
        }
        // lib.optionalAttrs (cfg.agent.credential != null) {
          PANDAR_AGENT_CREDENTIAL = cfg.agent.credential;
        }
        // cfg.agent.extraEnvironment;

        serviceConfig = {
          ExecStart = "${cfg.agent.package}/bin/pandar-agent";
          DynamicUser = true;
          StateDirectory = "pandar-agent";
          WorkingDirectory = "/var/lib/pandar-agent";
          Restart = "on-failure";
          RestartSec = "5s";
        }
        // lib.optionalAttrs (cfg.agent.environmentFile != null) {
          EnvironmentFile = cfg.agent.environmentFile;
        };
      };
    })

    (lib.mkIf authCfg.enable {
      systemd.services.pandar-auth = {
        description = "Pandar Better Auth issuer";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];

        environment = {
          HOSTNAME = authBind.host;
          PORT = authBind.port;
          PANDAR_AUTH_BASE_URL = authCfg.baseURL;
          PANDAR_AUTH_TRUSTED_ORIGINS = lib.concatStringsSep "," authCfg.trustedOrigins;
          PANDAR_AUTH_DASHBOARD_CALLBACK_URL = authCfg.dashboardCallbackUrl;
          PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL = authCfg.dashboardSignOutUrl;
          PANDAR_AUTH_DATABASE_FILE = toString authCfg.databaseFile;
          PANDAR_AUTH_JWT_MAX_AGE_SECONDS = toString authCfg.jwtMaxAgeSeconds;
        }
        // authCfg.extraEnvironment;

        serviceConfig = {
          ExecStartPre = "${authCfg.package}/bin/pandar-auth-migrate";
          ExecStart = "${authCfg.package}/bin/pandar-auth";
          DynamicUser = true;
          StateDirectory = "pandar-auth";
          WorkingDirectory = "/var/lib/pandar-auth";
          Restart = "on-failure";
          RestartSec = "5s";
        }
        // lib.optionalAttrs (authCfg.environmentFile != null) {
          EnvironmentFile = authCfg.environmentFile;
        };
      };
    })
  ];
}
