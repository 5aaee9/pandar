{
  lib,
  pkgs,
  pandarHubPackage,
  pandarWebPackage,
  pandarAgentPackage,
}:
let
  pandarModule = import ./nixos-module.nix {
    inherit pandarHubPackage pandarWebPackage pandarAgentPackage;
  };

  baseNode =
    { ... }:
    {
      imports = [ pandarModule ];

      services.pandar = {
        enable = true;
        web.enable = false;
      };

      environment.systemPackages = [ pkgs.curl ];
      system.stateVersion = "25.11";
    };

  hubAssertions = ''
    machine.wait_for_unit("pandar-hub.service")
    machine.wait_for_open_port(8080)
    machine.succeed("curl -fsS http://127.0.0.1:8080/healthz | grep -F 'ok'")
    machine.succeed("systemctl show pandar-hub.service --property=Environment | grep -F 'PANDAR_CONTROL_PLANE=in-process'")
  '';
in
{
  sqlite = pkgs.testers.runNixOSTest {
    name = "pandar-hub-sqlite";

    requiredFeatures.kvm = false;

    nodes.machine = baseNode;

    testScript = ''
      start_all()
      ${hubAssertions}
      machine.succeed("systemctl show pandar-hub.service --property=Environment | grep -F 'PANDAR_DATABASE_URL=sqlite:///var/lib/pandar-hub/pandar.db'")
      machine.succeed("test -s /var/lib/pandar-hub/pandar.db")
    '';
  };

  postgres = pkgs.testers.runNixOSTest {
    name = "pandar-hub-postgres";

    requiredFeatures.kvm = false;

    nodes.machine =
      { ... }:
      {
        imports = [ baseNode ];

        services.postgresql = {
          enable = true;
          enableTCPIP = true;
          ensureDatabases = [ "pandar" ];
          ensureUsers = [
            {
              name = "pandar";
              ensureDBOwnership = true;
            }
          ];
          authentication = lib.mkForce ''
            local all all trust
            host all all 127.0.0.1/32 trust
            host all all ::1/128 trust
          '';
        };

        services.pandar.hub.databaseUrl = "postgres://pandar@127.0.0.1/pandar";
      };

    testScript = ''
      start_all()
      machine.wait_for_unit("postgresql.service")
      ${hubAssertions}
      machine.succeed("systemctl show pandar-hub.service --property=Environment | grep -F 'PANDAR_DATABASE_URL=postgres://pandar@127.0.0.1/pandar'")
      machine.succeed("sudo -u postgres psql -d pandar -Atc \"select to_regclass('public.tenants')\" | grep -F tenants")
    '';
  };
}
