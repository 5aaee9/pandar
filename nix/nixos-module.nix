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
  sensitiveEnvironmentNames = [
    "APP_API_TOKEN"
    "APP_AUTH_BEARER_TOKEN"
    "BETTER_AUTH_SECRET"
    "PANDAR_AGENT_CREDENTIAL"
    "PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY"
    "PANDAR_AUTH_SMTP_PASSWORD"
    "PANDAR_DATABASE_URL"
    "PANDAR_PRINTERS"
    "PANDAR_PRINTER_ACCESS_CODE_KEY"
    "RESEND_API_KEY"
  ];
  hasSensitiveEnvironment =
    environment:
    lib.any (name: builtins.elem name sensitiveEnvironmentNames) (builtins.attrNames environment);
  isRuntimeEnvironmentFile = path: path == null || !lib.hasPrefix "/nix/store/" (toString path);
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
          default = "127.0.0.1:8080";
          description = "HTTP bind address for pandar-hub.";
        };

        grpcBind = lib.mkOption {
          type = lib.types.str;
          default = "127.0.0.1:50051";
          description = "gRPC bind address for pandar-hub agent connections.";
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

        environmentFile = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
          description = "Root-owned runtime systemd EnvironmentFile outside the Nix store containing PANDAR_DATABASE_URL, PANDAR_PRINTER_ACCESS_CODE_KEY, and other hub secrets.";
        };

        extraEnvironment = lib.mkOption {
          type = lib.types.attrsOf lib.types.str;
          default = { };
          description = "Non-sensitive extra environment variables for pandar-hub. Secrets must use environmentFile.";
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

        environmentFile = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
          description = "Optional root-owned runtime systemd EnvironmentFile outside the Nix store for frontend secrets such as APP_API_TOKEN or APP_AUTH_BEARER_TOKEN.";
        };

        extraEnvironment = lib.mkOption {
          type = lib.types.attrsOf lib.types.str;
          default = { };
          description = "Non-sensitive extra environment variables for pandar-web. Secrets must use environmentFile.";
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

        hubApiUrl = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Hub HTTP API URL passed through PANDAR_HUB_API_URL for saved printer connections and artifact downloads.";
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

        environmentFile = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
          description = "Root-owned runtime systemd EnvironmentFile outside the Nix store containing PANDAR_AGENT_CREDENTIAL and optional PANDAR_PRINTERS configuration.";
        };

        extraEnvironment = lib.mkOption {
          type = lib.types.attrsOf lib.types.str;
          default = { };
          description = "Non-sensitive extra environment variables for pandar-agent. Secrets must use environmentFile.";
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
        default = "http://127.0.0.1:3000/auth/betterauth/session";
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

      email = {
        magicLinkTtlSeconds = lib.mkOption {
          type = lib.types.ints.positive;
          default = 1800;
          description = "Email magic-link expiration in seconds passed through PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS.";
        };

        provider = lib.mkOption {
          type = lib.types.enum [
            "resend"
            "smtp"
          ];
          default = "resend";
          description = "Email delivery provider passed through PANDAR_AUTH_EMAIL_PROVIDER.";
        };

        from = lib.mkOption {
          type = lib.types.str;
          description = "From address passed through PANDAR_AUTH_EMAIL_FROM.";
        };

        brandName = lib.mkOption {
          type = lib.types.str;
          default = "Pandar";
          description = "Brand name used in magic-link email copy, passed through PANDAR_AUTH_EMAIL_BRAND_NAME.";
        };

        smtp = {
          host = lib.mkOption {
            type = lib.types.str;
            default = "";
            description = "SMTP host passed through PANDAR_AUTH_SMTP_HOST when email.provider is smtp.";
          };

          port = lib.mkOption {
            type = lib.types.ints.positive;
            default = 587;
            description = "SMTP port passed through PANDAR_AUTH_SMTP_PORT when email.provider is smtp.";
          };

          username = lib.mkOption {
            type = lib.types.str;
            default = "";
            description = "SMTP username passed through PANDAR_AUTH_SMTP_USERNAME when email.provider is smtp.";
          };

          tls = lib.mkOption {
            type = lib.types.enum [
              "starttls"
              "tls"
              "none"
            ];
            default = "starttls";
            description = "SMTP TLS mode passed through PANDAR_AUTH_SMTP_TLS when email.provider is smtp.";
          };
        };
      };

      environmentFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Root-owned runtime systemd EnvironmentFile outside the Nix store for Better Auth secrets such as BETTER_AUTH_SECRET, RESEND_API_KEY, or PANDAR_AUTH_SMTP_PASSWORD.";
      };

      extraEnvironment = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = { };
        description = "Non-sensitive extra environment variables for pandar-auth. Secrets must use environmentFile.";
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
          assertion = !cfg.enable || !cfg.hub.enable || cfg.hub.environmentFile != null;
          message = "services.pandar.hub.environmentFile is required for PANDAR_DATABASE_URL and PANDAR_PRINTER_ACCESS_CODE_KEY.";
        }
        {
          assertion = !cfg.enable || !cfg.hub.enable || isRuntimeEnvironmentFile cfg.hub.environmentFile;
          message = "services.pandar.hub.environmentFile must be a runtime path outside /nix/store.";
        }
        {
          assertion = !cfg.enable || !cfg.hub.enable || !hasSensitiveEnvironment cfg.hub.extraEnvironment;
          message = "services.pandar.hub.extraEnvironment cannot contain secrets; use services.pandar.hub.environmentFile.";
        }
        {
          assertion = !cfg.enable || !cfg.web.enable || isRuntimeEnvironmentFile cfg.web.environmentFile;
          message = "services.pandar.web.environmentFile must be a runtime path outside /nix/store.";
        }
        {
          assertion = !cfg.enable || !cfg.web.enable || !hasSensitiveEnvironment cfg.web.extraEnvironment;
          message = "services.pandar.web.extraEnvironment cannot contain secrets; use services.pandar.web.environmentFile.";
        }
        {
          assertion = !cfg.enable || !cfg.agent.enable || cfg.agent.environmentFile != null;
          message = "services.pandar.agent.environmentFile is required for PANDAR_AGENT_CREDENTIAL.";
        }
        {
          assertion = !cfg.enable || !cfg.agent.enable || isRuntimeEnvironmentFile cfg.agent.environmentFile;
          message = "services.pandar.agent.environmentFile must be a runtime path outside /nix/store.";
        }
        {
          assertion = !cfg.enable || !cfg.agent.enable || !hasSensitiveEnvironment cfg.agent.extraEnvironment;
          message = "services.pandar.agent.extraEnvironment cannot contain secrets; use services.pandar.agent.environmentFile.";
        }
        {
          assertion = !authCfg.enable || authBindParts != null;
          message = "services.pandar-auth.bind must be formatted as host:port.";
        }
        {
          assertion = !authCfg.enable || authCfg.environmentFile != null;
          message = "services.pandar-auth.environmentFile is required for Better Auth and email-provider secrets.";
        }
        {
          assertion = !authCfg.enable || isRuntimeEnvironmentFile authCfg.environmentFile;
          message = "services.pandar-auth.environmentFile must be a runtime path outside /nix/store.";
        }
        {
          assertion = !authCfg.enable || !hasSensitiveEnvironment authCfg.extraEnvironment;
          message = "services.pandar-auth.extraEnvironment cannot contain secrets; use services.pandar-auth.environmentFile.";
        }
        {
          assertion = !authCfg.enable || authCfg.email.from != "";
          message = "services.pandar-auth.email.from is required.";
        }

        {
          assertion = !authCfg.enable || authCfg.email.provider != "smtp" || authCfg.email.smtp.host != "";
          message = "services.pandar-auth.email.smtp.host is required when email.provider is \"smtp\".";
        }
        {
          assertion =
            !authCfg.enable || authCfg.email.provider != "smtp" || authCfg.email.smtp.username != "";
          message = "services.pandar-auth.email.smtp.username is required when email.provider is \"smtp\".";
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
        }
        // lib.optionalAttrs (cfg.hub.environmentFile != null) {
          EnvironmentFile = cfg.hub.environmentFile;
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
        }
        // lib.optionalAttrs (cfg.web.environmentFile != null) {
          EnvironmentFile = cfg.web.environmentFile;
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
        }
        // lib.optionalAttrs (cfg.agent.agentId != null) {
          PANDAR_AGENT_ID = cfg.agent.agentId;
        }
        // lib.optionalAttrs (cfg.agent.tenantId != null) {
          PANDAR_TENANT_ID = cfg.agent.tenantId;
        }

        // lib.optionalAttrs (cfg.agent.hubApiUrl != null) {
          PANDAR_HUB_API_URL = cfg.agent.hubApiUrl;
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
          PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS = toString authCfg.email.magicLinkTtlSeconds;
          PANDAR_AUTH_EMAIL_PROVIDER = authCfg.email.provider;
          PANDAR_AUTH_EMAIL_FROM = authCfg.email.from;
          PANDAR_AUTH_EMAIL_BRAND_NAME = authCfg.email.brandName;
        }
        // lib.optionalAttrs (authCfg.email.provider == "smtp") {
          PANDAR_AUTH_SMTP_HOST = authCfg.email.smtp.host;
          PANDAR_AUTH_SMTP_PORT = toString authCfg.email.smtp.port;
          PANDAR_AUTH_SMTP_USERNAME = authCfg.email.smtp.username;
          PANDAR_AUTH_SMTP_TLS = authCfg.email.smtp.tls;
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
