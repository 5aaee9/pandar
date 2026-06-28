{
  inputs,
  moduleWithSystem,
  ...
}:
{
  flake =
    let
      nixosModule = moduleWithSystem (
        { config }:
        import ./nixos-module.nix {
          pandarAgentPackage = config.packages.pandar-agent;
          pandarAuthPackage = config.packages.pandar-auth;
          pandarHubPackage = config.packages.pandar-hub;
          pandarWebPackage = config.packages.pandar-web;
        }
      );
    in
    {
      nixosModules = {
        default = nixosModule;
        pandar = nixosModule;
      };
    };

  perSystem =
    {
      config,
      pkgs,
      system,
      ...
    }:
    let
      inherit (pkgs) lib;
      fenixPkgs = inputs.fenix.packages.${system};

      toolchain = fenixPkgs.combine [
        (fenixPkgs.stable.withComponents [
          "cargo"
          "clippy"
          "rust-src"
          "rust-std"
          "rustc"
          "rustfmt"
        ])
      ];

      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;

      root = ./..;

      rustSrc = lib.cleanSourceWith {
        src = root;
        filter =
          path: type:
          let
            rel = lib.removePrefix "${toString root}/" (toString path);
          in
          rel == "Cargo.lock"
          || rel == "Cargo.toml"
          || rel == "crates"
          || lib.hasPrefix "crates/" rel
          || rel == "docs"
          || rel == "docs/superpowers"
          || rel == "docs/superpowers/specs"
          || lib.hasPrefix "docs/superpowers/specs/" rel
          || rel == "frontend"
          || rel == "frontend/plugin-local"
          || rel == "frontend/plugin-local/dist"
          || lib.hasPrefix "frontend/plugin-local/dist/" rel
          || rel == "proto"
          || lib.hasPrefix "proto/" rel;
      };

      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.protobuf
      ];

      buildInputs = [
        pkgs.openssl
      ];

      commonArgs = {
        src = rustSrc;
        version = "0.1.0";
        strictDeps = true;
        SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        inherit nativeBuildInputs buildInputs;
      };

      cargoArtifacts = craneLib.buildDepsOnly (
        commonArgs
        // {
          pname = "pandar-deps";
        }
      );

      buildRustPackage =
        pname: cargoExtraArgs:
        craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts cargoExtraArgs pname;
          }
        );

      pandar-hub = buildRustPackage "pandar-hub" "-p pandar-hub --bin pandar-hub";
      pandar-agent = buildRustPackage "pandar-agent" "-p pandar-agent --bin pandar-agent";
      pandar-cli = buildRustPackage "pandar-cli" "-p pandar-app --bin pandar";
      pandar-network-plugin = buildRustPackage "pandar-network-plugin" "-p pandar-network-plugin";

      pandarAuthLibraryPath = lib.makeLibraryPath [
        pkgs.sqlite
        pkgs.stdenv.cc.cc.lib
      ];
      frontendRoot = toString "${root}/frontend";
      frontendSource = lib.cleanSourceWith {
        src = "${root}/frontend";
        filter =
          path: _type:
          let
            relativePath = lib.removePrefix "${frontendRoot}/" (toString path);
          in
          relativePath != "auth" && !lib.hasPrefix "auth/" relativePath;
      };

      pandar-auth = pkgs.buildNpmPackage {
        pname = "pandar-auth";
        version = "0.1.0";
        src = lib.cleanSource "${root}/frontend/auth";
        npmDepsHash = "sha256-asLz6O5uL6BofKjh/Ra6vh8LqChwjAjobH4q9dF4La4=";

        nativeBuildInputs = [
          pkgs.makeWrapper
          pkgs.pkg-config
          pkgs.python3
        ];

        buildInputs = [
          pkgs.sqlite
        ];

        env = {
          NEXT_TELEMETRY_DISABLED = "1";
        };

        installPhase = ''
          runHook preInstall

          mkdir -p "$out/share/pandar-auth"
          cp -r .next/standalone/. "$out/share/pandar-auth/"
          cp -r .next/static "$out/share/pandar-auth/.next/static"

          mkdir -p "$out/share/pandar-auth/migrate-src"
          cp package.json package-lock.json tsconfig.json "$out/share/pandar-auth/migrate-src/"
          cp -r lib "$out/share/pandar-auth/migrate-src/lib"
          cp -r scripts "$out/share/pandar-auth/migrate-src/scripts"
          cp -r node_modules "$out/share/pandar-auth/migrate-src/node_modules"

          mkdir -p "$out/bin"
          makeWrapper ${pkgs.nodejs_24}/bin/node "$out/bin/pandar-auth" \
            --add-flags "$out/share/pandar-auth/server.js" \
            --set-default NODE_ENV production \
            --set-default PORT 3001 \
            --prefix LD_LIBRARY_PATH : ${pandarAuthLibraryPath}

          cat > "$out/bin/pandar-auth-migrate" <<EOF
          #!${pkgs.runtimeShell}
          set -euo pipefail
          export NODE_ENV="''${NODE_ENV:-production}"
          export LD_LIBRARY_PATH="${pandarAuthLibraryPath}''${LD_LIBRARY_PATH:+:}''${LD_LIBRARY_PATH:-}"
          cd "$out/share/pandar-auth/migrate-src"
          exec ${pkgs.nodejs_24}/bin/node node_modules/auth/dist/index.mjs migrate --config ./lib/auth.ts --yes "\$@"
          EOF
          chmod +x "$out/bin/pandar-auth-migrate"

          cat > "$out/share/pandar-auth/migrate-src/migrate-check.mjs" <<'EOF'
          import { getMigrations } from "better-auth/db/migration";
          import { auth } from "./lib/auth.ts";

          const { runMigrations } = await getMigrations(auth.options);
          await runMigrations();
          EOF

          runHook postInstall
        '';
      };

      pandar-web = pkgs.buildNpmPackage {
        pname = "pandar-web";
        version = "0.1.0";
        src = frontendSource;
        npmDepsHash = "sha256-RFtVgXp+lm4gPCzq/I0q0+yc1HhtumsNfWprNYuKvP0=";

        nativeBuildInputs = [
          pkgs.makeWrapper
        ];

        env = {
          NEXT_TELEMETRY_DISABLED = "1";
        };

        installPhase = ''
          runHook preInstall

          mkdir -p "$out/share/pandar-web"
          cp -r .next/standalone/. "$out/share/pandar-web/"
          cp -r .next/static "$out/share/pandar-web/.next/static"
          cp -r public "$out/share/pandar-web/public"

          mkdir -p "$out/bin"
          makeWrapper ${pkgs.nodejs_24}/bin/node "$out/bin/pandar-web" \
            --add-flags "$out/share/pandar-web/server.js" \
            --set-default NODE_ENV production \
            --set-default PORT 3000

          runHook postInstall
        '';
      };

      pandarNixosModuleCheck =
        let
          serviceNixosSystem = inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              (import ./nixos-module.nix {
                pandarAgentPackage = pandar-agent;
                pandarAuthPackage = pandar-auth;
                pandarHubPackage = pandar-hub;
                pandarWebPackage = pandar-web;
              })
              {
                services.pandar.enable = true;
                services.pandar.hub = {
                  controlPlane = "nats";
                  nats.mode = "service";
                  nats.subject = "pandar.test.control";
                };
                services.pandar.agent = {
                  enable = true;
                  agentId = "00000000-0000-0000-0000-000000000001";
                  tenantId = "00000000-0000-0000-0000-000000000002";
                  credential = "test-agent-credential";
                };
                system.stateVersion = "25.11";
              }
            ];
          };
          externalNixosSystem = inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              (import ./nixos-module.nix {
                pandarAgentPackage = pandar-agent;
                pandarAuthPackage = pandar-auth;
                pandarHubPackage = pandar-hub;
                pandarWebPackage = pandar-web;
              })
              {
                services.pandar.enable = true;
                services.pandar.hub = {
                  controlPlane = "nats";
                  nats = {
                    mode = "external";
                    url = "nats://broker.example:4222";
                    subject = "pandar.external.control";
                  };
                };
                system.stateVersion = "25.11";
              }
            ];
          };
          authNixosSystem = inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              (import ./nixos-module.nix {
                pandarAgentPackage = pandar-agent;
                pandarAuthPackage = pandar-auth;
                pandarHubPackage = pandar-hub;
                pandarWebPackage = pandar-web;
              })
              {
                services.pandar-auth = {
                  enable = true;
                  bind = "127.0.0.1:3001";
                  baseURL = "https://auth.example";
                  trustedOrigins = [ "https://app.example" ];
                  dashboardCallbackUrl = "https://app.example/auth/betterauth/callback";
                  dashboardSignOutUrl = "https://app.example/auth/betterauth/sign-out";
                  databaseFile = "/var/lib/pandar-auth/auth.db";
                  jwtMaxAgeSeconds = 3600;
                  email = {
                    magicLinkTtlSeconds = 1800;
                    provider = "smtp";
                    from = "Pandar <auth@example>";
                    brandName = "Pandar Cloud";
                    smtp = {
                      host = "smtp.example";
                      port = 465;
                      username = "pandar-auth";
                      tls = "tls";
                    };
                  };
                  environmentFile = "/run/secrets/pandar-auth.env";
                };
                system.stateVersion = "25.11";
              }
            ];
          };
          serviceHub = serviceNixosSystem.config.systemd.services.pandar-hub;
          serviceWeb = serviceNixosSystem.config.systemd.services.pandar-web;
          serviceAgent = serviceNixosSystem.config.systemd.services.pandar-agent;
          serviceNatsEnabled = if serviceNixosSystem.config.services.nats.enable then "1" else "0";
          externalHub = externalNixosSystem.config.systemd.services.pandar-hub;
          externalNatsEnabled = if externalNixosSystem.config.services.nats.enable then "1" else "0";
          authService = authNixosSystem.config.systemd.services.pandar-auth;
          authHubPresent = if authNixosSystem.config.systemd.services ? pandar-hub then "1" else "0";
        in
        pkgs.runCommand "pandar-nixos-module-check" { } ''
          test "${serviceHub.serviceConfig.ExecStart}" = "${pandar-hub}/bin/pandar-hub"
          test "${serviceWeb.serviceConfig.ExecStart}" = "${pandar-web}/bin/pandar-web"
          test "${serviceAgent.serviceConfig.ExecStart}" = "${pandar-agent}/bin/pandar-agent"
          test "${authService.serviceConfig.ExecStart}" = "${pandar-auth}/bin/pandar-auth"
          test "${authService.serviceConfig.ExecStartPre}" = "${pandar-auth}/bin/pandar-auth-migrate"
          test "${serviceNatsEnabled}" = "1"
          test "${serviceHub.environment.PANDAR_CONTROL_PLANE}" = "nats"
          test "${serviceHub.environment.PANDAR_NATS_URL}" = "nats://127.0.0.1:4222"
          test "${serviceHub.environment.PANDAR_NATS_SUBJECT}" = "pandar.test.control"
          test "${serviceWeb.environment.APP_API_URL}" = "http://127.0.0.1:8080"
          test "${serviceAgent.environment.PANDAR_HUB_GRPC_URL}" = "http://127.0.0.1:50051"
          test "${externalNatsEnabled}" = "0"
          test "${externalHub.environment.PANDAR_CONTROL_PLANE}" = "nats"
          test "${externalHub.environment.PANDAR_NATS_URL}" = "nats://broker.example:4222"
          test "${externalHub.environment.PANDAR_NATS_SUBJECT}" = "pandar.external.control"
          test "${authHubPresent}" = "0"
          test "${authService.environment.HOSTNAME}" = "127.0.0.1"
          test "${authService.environment.PORT}" = "3001"
          test "${authService.environment.PANDAR_AUTH_BASE_URL}" = "https://auth.example"
          test "${authService.environment.PANDAR_AUTH_TRUSTED_ORIGINS}" = "https://app.example"
          test "${authService.environment.PANDAR_AUTH_DASHBOARD_CALLBACK_URL}" = "https://app.example/auth/betterauth/callback"
          test "${authService.environment.PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL}" = "https://app.example/auth/betterauth/sign-out"
          test "${authService.environment.PANDAR_AUTH_DATABASE_FILE}" = "/var/lib/pandar-auth/auth.db"
          test "${authService.environment.PANDAR_AUTH_JWT_MAX_AGE_SECONDS}" = "3600"
          test "${authService.environment.PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS}" = "1800"
          test "${authService.environment.PANDAR_AUTH_EMAIL_PROVIDER}" = "smtp"
          test "${authService.environment.PANDAR_AUTH_EMAIL_FROM}" = "Pandar <auth@example>"
          test "${authService.environment.PANDAR_AUTH_EMAIL_BRAND_NAME}" = "Pandar Cloud"
          test "${authService.environment.PANDAR_AUTH_SMTP_HOST}" = "smtp.example"
          test "${authService.environment.PANDAR_AUTH_SMTP_PORT}" = "465"
          test "${authService.environment.PANDAR_AUTH_SMTP_USERNAME}" = "pandar-auth"
          test "${authService.environment.PANDAR_AUTH_SMTP_TLS}" = "tls"
          test "${authService.serviceConfig.EnvironmentFile}" = "/run/secrets/pandar-auth.env"
          touch "$out"
        '';

      pandarAuthMigrateCheck = pkgs.runCommand "pandar-auth-migrate-check" { } ''
        export BETTER_AUTH_SECRET="pandar-auth-test-secret"
        export PANDAR_AUTH_BASE_URL="http://127.0.0.1:3001"
        export PANDAR_AUTH_TRUSTED_ORIGINS="http://127.0.0.1:3000"
        export PANDAR_AUTH_DASHBOARD_CALLBACK_URL="http://127.0.0.1:3000/auth/betterauth/callback"
        export PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL="http://127.0.0.1:3000/auth/betterauth/sign-out"
        export PANDAR_AUTH_DATABASE_FILE="$TMPDIR/auth.db"
        export PANDAR_AUTH_JWT_MAX_AGE_SECONDS="3600"
        export PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS="1800"
        export PANDAR_AUTH_EMAIL_PROVIDER="resend"
        export PANDAR_AUTH_EMAIL_FROM="Pandar <auth@example.invalid>"
        export PANDAR_AUTH_EMAIL_BRAND_NAME="Pandar"
        export RESEND_API_KEY="re_test_key"

        cd ${pandar-auth}/share/pandar-auth
        LD_LIBRARY_PATH=${pandarAuthLibraryPath} ${pkgs.nodejs_24}/bin/node -e 'require("better-sqlite3")'

        cd ${pandar-auth}/share/pandar-auth/migrate-src
        ${pkgs.nodejs_24}/bin/node migrate-check.mjs
        ${lib.getExe pkgs.sqlite} "$PANDAR_AUTH_DATABASE_FILE" ".tables" | grep -F jwks
        ${lib.getExe pkgs.sqlite} "$PANDAR_AUTH_DATABASE_FILE" ".tables" | grep -F passkey
        touch "$out"
      '';

      pandarAuthJwtSmokeCheck = pkgs.runCommand "pandar-auth-jwt-smoke-check" { } ''
        cd ${pandar-auth}/share/pandar-auth/migrate-src
        export BETTER_AUTH_SECRET="pandar-auth-smoke-secret-at-least-32-chars"
        export PANDAR_AUTH_BASE_URL="http://127.0.0.1:3001"
        export PANDAR_AUTH_TRUSTED_ORIGINS="http://127.0.0.1:3000"
        export PANDAR_AUTH_DASHBOARD_CALLBACK_URL="http://127.0.0.1:3000/auth/betterauth/callback"
        export PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL="http://127.0.0.1:3000/auth/betterauth/sign-out"
        export PANDAR_AUTH_DATABASE_FILE="$TMPDIR/pandar-auth-smoke.db"
        export PANDAR_AUTH_JWT_MAX_AGE_SECONDS="3600"
        export PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS="1800"
        export PANDAR_AUTH_EMAIL_PROVIDER="resend"
        export PANDAR_AUTH_EMAIL_FROM="Pandar <auth@example.invalid>"
        export PANDAR_AUTH_EMAIL_BRAND_NAME="Pandar"
        export RESEND_API_KEY="re_test_key"
        LD_LIBRARY_PATH=${pandarAuthLibraryPath} ${pkgs.nodejs_24}/bin/node \
          --experimental-strip-types \
          scripts/smoke-jwt-and-registration.mjs
        touch "$out"
      '';

      pandarAuthCookieSmokeCheck = pkgs.runCommand "pandar-auth-cookie-smoke-check" { } ''
        cd ${frontendSource}
        ${pkgs.nodejs_24}/bin/node \
          --experimental-strip-types \
          app/auth/betterauth/cookie.smoke.mjs
        touch "$out"
      '';

      pandarWebAuthRedirectSmokeCheck = pkgs.runCommand "pandar-web-auth-redirect-smoke-check" { } ''
        cd ${frontendSource}
        ${pkgs.nodejs_24}/bin/node \
          --experimental-strip-types \
          app/auth-redirect.smoke.mjs
        touch "$out"
      '';

      pandarNixosOptionsDoc =
        let
          nixosSystem = inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              (import ./nixos-module.nix {
                pandarAgentPackage = pandar-agent;
                pandarAuthPackage = pandar-auth;
                pandarHubPackage = pandar-hub;
                pandarWebPackage = pandar-web;
              })
              {
                system.stateVersion = "25.11";
              }
            ];
          };
          optionsDoc = pkgs.nixosOptionsDoc {
            options = {
              services.pandar = nixosSystem.options.services.pandar;
              services.pandar-auth = nixosSystem.options.services.pandar-auth;
            };
          };
        in
        pkgs.runCommand "pandar-nixos-options.md" { } ''
          doc="$TMPDIR/options.md"
          cat > "$doc" <<'EOF'
          # NixOS Module Options

          Generated from `nixosModules.default`.

          EOF
          awk '
            /^\*Declared by:\*/ { skip = 1; next }
            skip && /^ - / { next }
            skip && /^$/ { skip = 0; next }
            { print }
          ' ${optionsDoc.optionsCommonMark} >> "$doc"
          sed -i -e :a -e '/^\n*$/{$d;N;ba' -e '}' "$doc"
          ${lib.getExe pkgs.prettier} --write "$doc"
          cp "$doc" "$out"
        '';

      pandarNixosOptionsDocCheck = pkgs.runCommand "pandar-nixos-options-doc-check" { } ''
        diff -u ${pandarNixosOptionsDoc} ${root}/docs/deployment/nixos/options.md
        touch "$out"
      '';

      pandarNixosTests = import ./nixos-tests.nix {
        inherit lib pkgs;
        pandarAuthPackage = pandar-auth;
        pandarHubPackage = pandar-hub;
        pandarWebPackage = pandar-web;
        pandarAgentPackage = pandar-agent;
      };

    in
    {
      treefmt = import ./treefmt.nix;

      packages = {
        default = pandar-hub;
        inherit
          pandar-hub
          pandar-agent
          pandar-cli
          pandar-network-plugin
          pandar-auth
          pandar-web
          ;
      };

      checks = {
        inherit
          pandar-hub
          pandar-agent
          pandar-cli
          pandar-network-plugin
          pandar-auth
          pandar-web
          ;

        pandar-auth-migrate = pandarAuthMigrateCheck;
        pandar-auth-jwt-smoke = pandarAuthJwtSmokeCheck;
        pandar-auth-cookie-smoke = pandarAuthCookieSmokeCheck;
        pandar-web-auth-redirect-smoke = pandarWebAuthRedirectSmokeCheck;
        pandar-nixos-module = pandarNixosModuleCheck;
        pandar-nixos-options-doc = pandarNixosOptionsDocCheck;
        pandar-nixos-test-sqlite = pandarNixosTests.sqlite;
        pandar-nixos-test-postgres = pandarNixosTests.postgres;

        pandar-clippy = craneLib.cargoClippy (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "pandar-clippy";
            cargoClippyExtraArgs = "--workspace --all-targets -- --deny warnings";
          }
        );

        pandar-nextest = craneLib.cargoNextest (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "pandar-nextest";
            cargoNextestExtraArgs = "--workspace";
          }
        );

        pandar-fmt = craneLib.cargoFmt {
          src = rustSrc;
          version = "0.1.0";
          pname = "pandar-fmt";
        };
      };

      devShells.default = craneLib.devShell {
        checks = config.checks;

        packages = [
          config.treefmt.build.wrapper
          pkgs.cargo-nextest
          pkgs.lefthook
          pkgs.nodejs_24
          pkgs.pkg-config
          pkgs.protobuf
          fenixPkgs.rust-analyzer
          toolchain
        ];
      };

      formatter = config.treefmt.build.wrapper;
    };
}
