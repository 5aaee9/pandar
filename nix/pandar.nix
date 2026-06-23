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

      pandar-web = pkgs.buildNpmPackage {
        pname = "pandar-web";
        version = "0.1.0";
        src = lib.cleanSource "${root}/frontend";
        npmDepsHash = "sha256-lf54i1KOPL3H9zl5iIWkk13S5JSrDTVnxYcsalVI3WU=";

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
          serviceHub = serviceNixosSystem.config.systemd.services.pandar-hub;
          serviceWeb = serviceNixosSystem.config.systemd.services.pandar-web;
          serviceAgent = serviceNixosSystem.config.systemd.services.pandar-agent;
          serviceNatsEnabled = if serviceNixosSystem.config.services.nats.enable then "1" else "0";
          externalHub = externalNixosSystem.config.systemd.services.pandar-hub;
          externalNatsEnabled = if externalNixosSystem.config.services.nats.enable then "1" else "0";
        in
        pkgs.runCommand "pandar-nixos-module-check" { } ''
          test "${serviceHub.serviceConfig.ExecStart}" = "${pandar-hub}/bin/pandar-hub"
          test "${serviceWeb.serviceConfig.ExecStart}" = "${pandar-web}/bin/pandar-web"
          test "${serviceAgent.serviceConfig.ExecStart}" = "${pandar-agent}/bin/pandar-agent"
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
          touch "$out"
        '';

      pandarNixosOptionsDoc =
        let
          nixosSystem = inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              (import ./nixos-module.nix {
                pandarAgentPackage = pandar-agent;
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
          cp "$doc" "$out"
        '';

      pandarNixosOptionsDocCheck = pkgs.runCommand "pandar-nixos-options-doc-check" { } ''
        diff -u ${pandarNixosOptionsDoc} ${root}/docs/deployment/nixos/options.md
        touch "$out"
      '';

      formatter = pkgs.writeShellApplication {
        name = "pandar-nixfmt";
        runtimeInputs = [
          pkgs.nixfmt
        ];
        text = ''
          if [ "$#" -eq 0 ]; then
            set -- flake.nix nix/*.nix
          fi
          exec nixfmt "$@"
        '';
      };
    in
    {
      packages = {
        default = pandar-hub;
        inherit
          pandar-hub
          pandar-agent
          pandar-cli
          pandar-network-plugin
          pandar-web
          ;
      };

      checks = {
        inherit
          pandar-hub
          pandar-agent
          pandar-network-plugin
          pandar-web
          ;

        pandar-nixos-module = pandarNixosModuleCheck;
        pandar-nixos-options-doc = pandarNixosOptionsDocCheck;

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
          pkgs.cargo-nextest
          pkgs.nodejs_24
          pkgs.pkg-config
          pkgs.protobuf
          fenixPkgs.rust-analyzer
          toolchain
        ];
      };

      inherit formatter;
    };
}
