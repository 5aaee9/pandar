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

      version = "0.2.1";

      rustSrc = lib.cleanSourceWith {
        src = root;
        filter =
          path: type:
          let
            rel = lib.removePrefix "${toString root}/" (toString path);
          in
          rel == ".config"
          || rel == ".config/nextest.toml"
          || rel == "Cargo.lock"
          || rel == "Cargo.toml"
          || rel == "studio-abi-profiles.json"
          || rel == "contracts"
          || lib.hasPrefix "contracts/" rel
          || rel == "crates"
          || lib.hasPrefix "crates/" rel
          || rel == "frontend"
          || rel == "frontend/plugin-local"
          || rel == "frontend/plugin-local/dist"
          || lib.hasPrefix "frontend/plugin-local/dist/" rel
          || rel == "proto"
          || lib.hasPrefix "proto/" rel
          || rel == "tools"
          || rel == "tools/pandar-quality"
          || lib.hasPrefix "tools/pandar-quality/" rel;
      };

      moduleSizeSrc = lib.cleanSourceWith {
        src = root;
        filter =
          path: type:
          let
            rel = lib.removePrefix "${toString root}/" (toString path);
            generated = lib.any (
              component:
              builtins.elem component [
                "node_modules"
                ".next"
                ".gradle"
                "build"
                "dist"
                "generated"
                "out"
                "target"
              ]
            ) (lib.splitString "/" rel);
          in
          !generated
          && (
            rel == "crates"
            || lib.hasPrefix "crates/" rel
            || rel == "frontend"
            || lib.hasPrefix "frontend/" rel
            || rel == "mobile"
            || rel == "mobile/android"
            || lib.hasPrefix "mobile/android/" rel
          );
      };

      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.protobuf
        # openssl-sys builds its vendored OpenSSL from source; the Configure
        # script is a Perl program.
        pkgs.perl
      ]
      ++ lib.optional (system == "aarch64-linux") pkgs.lld;

      buildInputs = [
        pkgs.openssl
      ];

      commonArgs = {
        src = rustSrc;
        inherit version;
        strictDeps = true;
        SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        inherit nativeBuildInputs buildInputs;
      }
      // lib.optionalAttrs (system == "aarch64-linux") {
        # On aarch64-unknown-linux-gnu, rustc links cdylibs with the system bfd
        # linker and auto-generates an anonymous version script (with `local: *`)
        # that conflicts with pandar-network-plugin's build.rs export map
        # ("anonymous version tag cannot be combined with other version tags").
        # Route the final link through lld instead: it merges the two version
        # scripts and keeps the build.rs exports (129 bambu_network_*/ft_*
        # symbols), matching x86_64-linux's behavior. `lld` (added to
        # nativeBuildInputs above) provides the `ld.lld` that -fuse-ld selects.
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=lld";
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
      pandar-agent-unwrapped = buildRustPackage "pandar-agent-unwrapped" "-p pandar-agent --bin pandar-agent";
      pandar-agent = pkgs.runCommand "pandar-agent-${version}" { nativeBuildInputs = [ pkgs.makeWrapper ]; } ''
        mkdir -p "$out/bin"
        makeWrapper ${pandar-agent-unwrapped}/bin/pandar-agent "$out/bin/pandar-agent" \
          --set-default PANDAR_FFMPEG_PATH ${lib.getExe pkgs.ffmpeg} \
          --prefix PATH : ${lib.makeBinPath [ pkgs.ffmpeg ]}
      '';
      pandar-cli = buildRustPackage "pandar-cli" "-p pandar-app --bin pandar";
      pandar-quality = buildRustPackage "pandar-quality" "-p pandar-quality --bin pandar-quality";
      pandar-network-plugin = craneLib.buildPackage (
        commonArgs
        // {
          inherit cargoArtifacts;
          pname = "pandar-network-plugin";
          cargoExtraArgs = "-p pandar-network-plugin";
          # External-checkout contract tests run outside the hermetic package build.
          doCheck = false;
        }
      );

      pandarModuleSizeCheck =
        pkgs.runCommand "pandar-module-size" { nativeBuildInputs = [ pandar-quality ]; }
          ''
            pandar-quality module-size ${moduleSizeSrc}
            touch "$out"
          '';

      pandarAuthLibraryPath = lib.makeLibraryPath [
        pkgs.sqlite
        pkgs.stdenv.cc.cc.lib
      ];
      frontendWorkspaceRoot = toString root;
      frontendWorkspaceSource = lib.cleanSourceWith {
        src = root;
        filter =
          path: type:
          let
            relativePath = lib.removePrefix "${frontendWorkspaceRoot}/" (toString path);
            isGenerated =
              relativePath == "node_modules"
              || lib.hasPrefix "node_modules/" relativePath
              || relativePath == ".next"
              || lib.hasPrefix ".next/" relativePath
              || relativePath == "frontend/node_modules"
              || lib.hasPrefix "frontend/node_modules/" relativePath
              || relativePath == "frontend/.next"
              || lib.hasPrefix "frontend/.next/" relativePath
              || relativePath == "frontend/out"
              || lib.hasPrefix "frontend/out/" relativePath
              || relativePath == "frontend/tsconfig.tsbuildinfo"
              || relativePath == "frontend/auth/node_modules"
              || lib.hasPrefix "frontend/auth/node_modules/" relativePath
              || relativePath == "frontend/auth/.next"
              || lib.hasPrefix "frontend/auth/.next/" relativePath
              || relativePath == "frontend/auth/out"
              || lib.hasPrefix "frontend/auth/out/" relativePath
              || relativePath == "frontend/auth/tsconfig.tsbuildinfo"
              || relativePath == "frontend/plugin-local/node_modules"
              || lib.hasPrefix "frontend/plugin-local/node_modules/" relativePath
              || relativePath == "frontend/plugin-local/tsconfig.tsbuildinfo";
          in
          !isGenerated
          && (
            relativePath == "package.json"
            || relativePath == "package-lock.json"
            || relativePath == "contracts"
            || lib.hasPrefix "contracts/" relativePath
            || relativePath == "frontend"
            || lib.hasPrefix "frontend/" relativePath
          );
      };
      frontendSource = "${frontendWorkspaceSource}/frontend";

      pandar-auth = pkgs.buildNpmPackage {
        pname = "pandar-auth";
        inherit version;
        src = frontendWorkspaceSource;
        npmWorkspace = "pandar-auth";
        npmDepsFetcherVersion = 2;
        npmDepsHash = "sha256-Z9zcxOoS+qYmlMmqboBWvZ5OjN8H0LDSTyfpYfbObRI=";

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

          test -f package.json
          test -f package-lock.json
          test -f frontend/auth/package.json
          test -d frontend/auth/.next/standalone
          test -f frontend/auth/.next/standalone/frontend/auth/server.js
          test -f frontend/lib/utils.ts
          test -d node_modules

          mkdir -p "$out/share/pandar-auth"
          cp -r frontend/auth/.next/standalone/. "$out/share/pandar-auth/"
          cp -r "$out/share/pandar-auth/frontend/auth/." "$out/share/pandar-auth/"
          ln -s share/pandar-auth/node_modules "$out/node_modules"
          cp -r frontend/auth/.next/static "$out/share/pandar-auth/.next/static"

          mkdir -p "$out/share/pandar-auth/migrate-src"
          cp frontend/auth/package.json frontend/auth/tsconfig.json "$out/share/pandar-auth/migrate-src/"
          cp -r frontend/auth/lib "$out/share/pandar-auth/migrate-src/lib"
          cp -r frontend/auth/scripts "$out/share/pandar-auth/migrate-src/scripts"
          cp -r node_modules "$out/share/pandar-auth/migrate-src/node_modules"
          mkdir -p "$out/share/pandar-auth/migrate-src/frontend"
          mkdir -p "$out/share/pandar-auth/migrate-src/frontend/auth"
          mkdir -p "$out/share/pandar-auth/migrate-src/frontend/plugin-local"
          cp frontend/package.json "$out/share/pandar-auth/migrate-src/frontend/package.json"
          cp frontend/auth/package.json "$out/share/pandar-auth/migrate-src/frontend/auth/package.json"
          cp frontend/plugin-local/package.json "$out/share/pandar-auth/migrate-src/frontend/plugin-local/package.json"
          ln -s ../migrate-src/node_modules/clsx "$out/share/pandar-auth/node_modules/clsx"
          ln -s ../migrate-src/node_modules/tailwind-merge "$out/share/pandar-auth/node_modules/tailwind-merge"
          mkdir -p "$out/share/pandar-auth/lib"
          cp frontend/lib/utils.ts "$out/share/pandar-auth/lib/utils.ts"
          ln -s utils.ts "$out/share/pandar-auth/lib/utils"

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
        inherit version;
        src = frontendWorkspaceSource;
        npmDepsFetcherVersion = 2;
        npmDepsHash = "sha256-Z9zcxOoS+qYmlMmqboBWvZ5OjN8H0LDSTyfpYfbObRI=";
        npmBuildScript = "build:web";

        nativeBuildInputs = [
          pkgs.makeWrapper
        ];

        env = {
          NEXT_TELEMETRY_DISABLED = "1";
        };

        installPhase = ''
          runHook preInstall

          test -f package.json
          test -f package-lock.json
          test -f frontend/package.json
          test -f frontend/plugin-local/dist/index.html
          test -d frontend/.next/standalone
          test -f frontend/.next/standalone/frontend/server.js
          test -d node_modules

          mkdir -p "$out/share/pandar-web"
          cp -r frontend/.next/standalone/. "$out/share/pandar-web/"
          cp -r "$out/share/pandar-web/frontend/." "$out/share/pandar-web/"
          cp -r frontend/.next/static "$out/share/pandar-web/.next/static"
          cp -r frontend/public "$out/share/pandar-web/public"

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
                  environmentFile = "/run/secrets/pandar-hub.env";
                };
                services.pandar.web.environmentFile = "/run/secrets/pandar-web.env";
                services.pandar.agent = {
                  enable = true;
                  hubApiUrl = "http://127.0.0.1:8080";
                  agentId = "00000000-0000-0000-0000-000000000001";
                  tenantId = "00000000-0000-0000-0000-000000000002";
                  environmentFile = "/run/secrets/pandar-agent.env";
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
                  environmentFile = "/run/secrets/pandar-hub.env";
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
                  dashboardSignOutUrl = "https://app.example/auth/betterauth/session";
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
          unsafeNixosSystem = inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              (import ./nixos-module.nix {
                pandarAgentPackage = pandar-agent;
                pandarAuthPackage = pandar-auth;
                pandarHubPackage = pandar-hub;
                pandarWebPackage = pandar-web;
              })
              {
                services.pandar = {
                  enable = true;
                  hub = {
                    environmentFile = pkgs.writeText "unsafe-pandar-hub.env" "test-only";
                    extraEnvironment.PANDAR_DATABASE_URL = "postgres://secret@example/pandar";
                  };
                  web.extraEnvironment.APP_API_TOKEN = "test-only";
                  agent = {
                    enable = true;
                    environmentFile = "/run/secrets/pandar-agent.env";
                    extraEnvironment.PANDAR_AGENT_CREDENTIAL = "test-only";
                  };
                };
                services.pandar-auth = {
                  enable = true;
                  environmentFile = "/run/secrets/pandar-auth.env";
                  email.from = "Pandar <auth@example>";
                  extraEnvironment.BETTER_AUTH_SECRET = "test-only";
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
          hubDatabaseEnvironmentPresent = if serviceHub.environment ? PANDAR_DATABASE_URL then "1" else "0";
          agentCredentialEnvironmentPresent =
            if serviceAgent.environment ? PANDAR_AGENT_CREDENTIAL then "1" else "0";
          agentPrintersEnvironmentPresent = if serviceAgent.environment ? PANDAR_PRINTERS then "1" else "0";
          hubDatabaseOptionPresent =
            if serviceNixosSystem.options.services.pandar.hub ? databaseUrl then "1" else "0";
          agentCredentialOptionPresent =
            if serviceNixosSystem.options.services.pandar.agent ? credential then "1" else "0";
          agentPrintersOptionPresent =
            if serviceNixosSystem.options.services.pandar.agent ? printers then "1" else "0";
          unsafeAssertionFailed =
            message:
            lib.any (
              assertion: !assertion.assertion && assertion.message == message
            ) unsafeNixosSystem.config.assertions;
          storeEnvironmentFileRejected =
            if
              unsafeAssertionFailed "services.pandar.hub.environmentFile must be a runtime path outside /nix/store."
            then
              "1"
            else
              "0";
          hubSecretEnvironmentRejected =
            if
              unsafeAssertionFailed "services.pandar.hub.extraEnvironment cannot contain secrets; use services.pandar.hub.environmentFile."
            then
              "1"
            else
              "0";
          webSecretEnvironmentRejected =
            if
              unsafeAssertionFailed "services.pandar.web.extraEnvironment cannot contain secrets; use services.pandar.web.environmentFile."
            then
              "1"
            else
              "0";
          agentSecretEnvironmentRejected =
            if
              unsafeAssertionFailed "services.pandar.agent.extraEnvironment cannot contain secrets; use services.pandar.agent.environmentFile."
            then
              "1"
            else
              "0";
          authSecretEnvironmentRejected =
            if
              unsafeAssertionFailed "services.pandar-auth.extraEnvironment cannot contain secrets; use services.pandar-auth.environmentFile."
            then
              "1"
            else
              "0";
        in
        pkgs.runCommand "pandar-nixos-module-check" { } ''
          test "${serviceHub.serviceConfig.ExecStart}" = "${pandar-hub}/bin/pandar-hub"
          test "${serviceWeb.serviceConfig.ExecStart}" = "${pandar-web}/bin/pandar-web"
          test "${serviceAgent.serviceConfig.ExecStart}" = "${pandar-agent}/bin/pandar-agent"
          grep -F 'PANDAR_FFMPEG_PATH' "${serviceAgent.serviceConfig.ExecStart}"
          grep -F '${lib.getExe pkgs.ffmpeg}' "${serviceAgent.serviceConfig.ExecStart}"
          test "${authService.serviceConfig.ExecStart}" = "${pandar-auth}/bin/pandar-auth"
          test "${authService.serviceConfig.ExecStartPre}" = "${pandar-auth}/bin/pandar-auth-migrate"
          test "${serviceNatsEnabled}" = "1"
          test "${serviceHub.environment.PANDAR_CONTROL_PLANE}" = "nats"
          test "${serviceHub.environment.PANDAR_NATS_URL}" = "nats://127.0.0.1:4222"
          test "${serviceHub.environment.PANDAR_NATS_SUBJECT}" = "pandar.test.control"
          test "${serviceHub.serviceConfig.EnvironmentFile}" = "/run/secrets/pandar-hub.env"
          test "${serviceWeb.serviceConfig.EnvironmentFile}" = "/run/secrets/pandar-web.env"
          test "${serviceWeb.environment.APP_API_URL}" = "http://127.0.0.1:8080"
          test "${serviceAgent.environment.PANDAR_HUB_GRPC_URL}" = "http://127.0.0.1:50051"
          test "${serviceAgent.environment.PANDAR_HUB_API_URL}" = "http://127.0.0.1:8080"
          test "${serviceAgent.serviceConfig.EnvironmentFile}" = "/run/secrets/pandar-agent.env"
          test "${hubDatabaseEnvironmentPresent}" = "0"
          test "${agentCredentialEnvironmentPresent}" = "0"
          test "${agentPrintersEnvironmentPresent}" = "0"
          test "${hubDatabaseOptionPresent}" = "0"
          test "${agentCredentialOptionPresent}" = "0"
          test "${agentPrintersOptionPresent}" = "0"
          test "${storeEnvironmentFileRejected}" = "1"
          test "${hubSecretEnvironmentRejected}" = "1"
          test "${webSecretEnvironmentRejected}" = "1"
          test "${agentSecretEnvironmentRejected}" = "1"
          test "${authSecretEnvironmentRejected}" = "1"
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
          test "${authService.environment.PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL}" = "https://app.example/auth/betterauth/session"
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
        export PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL="http://127.0.0.1:3000/auth/betterauth/session"
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
        ${pkgs.nodejs_24}/bin/node --experimental-strip-types -e 'await import("./lib/utils.ts")'
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
        export PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL="http://127.0.0.1:3000/auth/betterauth/session"
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
          scripts/auth/betterauth/cookie.smoke.mjs
        touch "$out"
      '';

      pandarWebAuthRedirectSmokeCheck = pkgs.runCommand "pandar-web-auth-redirect-smoke-check" { } ''
        cd ${frontendSource}
        ${pkgs.nodejs_24}/bin/node \
          --experimental-strip-types \
          scripts/auth-redirect.smoke.mjs
        ${pkgs.nodejs_24}/bin/node \
          --experimental-strip-types \
          scripts/auth/betterauth/callback.smoke.mjs
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
        pandar-module-size = pandarModuleSizeCheck;
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
            # These contract gates require an external pinned BambuStudio Git checkout.
            cargoNextestExtraArgs = "--workspace -E 'not (binary(studio_print_contract_red) | binary(studio_projection_contract) | binary(personal_presets))'";
          }
        );

        pandar-fmt = craneLib.cargoFmt {
          src = rustSrc;
          inherit version;
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
