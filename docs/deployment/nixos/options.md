# NixOS Module Options

Generated from `nixosModules.default`.

## services\.pandar\.enable



Whether to enable Pandar hub and web services\.



*Type:*
boolean



*Default:*

```nix
false
```



*Example:*

```nix
true
```



## services\.pandar\.agent\.enable



Whether to run pandar-agent when Pandar is enabled\.



*Type:*
boolean



*Default:*

```nix
false
```



## services\.pandar\.agent\.package



pandar-agent package to run\.



*Type:*
package



*Default:*

```nix
<derivation pandar-agent-0.1.0>
```



## services\.pandar\.agent\.agentId

Agent ID passed through PANDAR_AGENT_ID\.



*Type:*
null or string



*Default:*

```nix
null
```



## services\.pandar\.agent\.artifactRoot



Artifact root passed through PANDAR_ARTIFACT_ROOT\.



*Type:*
absolute path



*Default:*

```nix
"/var/lib/pandar-agent/artifacts"
```



## services\.pandar\.agent\.credential



Agent credential passed through PANDAR_AGENT_CREDENTIAL\.



*Type:*
null or string



*Default:*

```nix
null
```



## services\.pandar\.agent\.environmentFile



Optional systemd EnvironmentFile for agent secrets\.



*Type:*
null or absolute path



*Default:*

```nix
null
```



## services\.pandar\.agent\.extraEnvironment



Extra environment variables for pandar-agent\.



*Type:*
attribute set of string



*Default:*

```nix
{ }
```



## services\.pandar\.agent\.hubGrpcUrl



Hub gRPC URL passed through PANDAR_HUB_GRPC_URL\.



*Type:*
string



*Default:*

```nix
"http://127.0.0.1:50051"
```



## services\.pandar\.agent\.name



Agent name passed through PANDAR_AGENT_NAME\.



*Type:*
string



*Default:*

```nix
"local-agent"
```



## services\.pandar\.agent\.printers



Printer endpoint JSON passed through PANDAR_PRINTERS\.



*Type:*
string



*Default:*

```nix
"[]"
```



## services\.pandar\.agent\.tenantId



Tenant ID passed through PANDAR_TENANT_ID\.



*Type:*
null or string



*Default:*

```nix
null
```



## services\.pandar\.hub\.enable



Whether to run pandar-hub when Pandar is enabled\.



*Type:*
boolean



*Default:*

```nix
true
```



## services\.pandar\.hub\.package



pandar-hub package to run\.



*Type:*
package



*Default:*

```nix
<derivation pandar-hub-0.1.0>
```



## services\.pandar\.hub\.bind



HTTP bind address for pandar-hub\.



*Type:*
string



*Default:*

```nix
"0.0.0.0:8080"
```



## services\.pandar\.hub\.controlPlane



Hub control plane passed through PANDAR_CONTROL_PLANE\.



*Type:*
one of “in-process”, “nats”



*Default:*

```nix
"in-process"
```



## services\.pandar\.hub\.databaseUrl



Database URL passed through PANDAR_DATABASE_URL\.



*Type:*
string



*Default:*

```nix
"sqlite:///var/lib/pandar-hub/pandar.db"
```



## services\.pandar\.hub\.extraEnvironment



Extra environment variables for pandar-hub\.



*Type:*
attribute set of string



*Default:*

```nix
{ }
```



## services\.pandar\.hub\.grpcBind



gRPC bind address for pandar-hub agent connections\.



*Type:*
string



*Default:*

```nix
"0.0.0.0:50051"
```



## services\.pandar\.hub\.nats\.mode



NATS source for the hub control plane\. ` external ` uses ` services.pandar.hub.nats.url `;
` service ` enables the local NixOS NATS service and points the hub at it\.



*Type:*
one of “external”, “service”



*Default:*

```nix
"external"
```



## services\.pandar\.hub\.nats\.subject



Optional NATS subject passed through PANDAR_NATS_SUBJECT\.



*Type:*
null or string



*Default:*

```nix
null
```



## services\.pandar\.hub\.nats\.url



External NATS URL passed through PANDAR_NATS_URL when the hub uses the NATS control plane\.



*Type:*
null or string



*Default:*

```nix
null
```



## services\.pandar\.hub\.spoolDir



Artifact spool directory passed through PANDAR_SPOOL_DIR\.



*Type:*
absolute path



*Default:*

```nix
"/var/lib/pandar-hub/spool"
```



## services\.pandar\.web\.enable



Whether to run pandar-web when Pandar is enabled\.



*Type:*
boolean



*Default:*

```nix
true
```



## services\.pandar\.web\.package



pandar-web package to run\.



*Type:*
package



*Default:*

```nix
<derivation pandar-web-0.1.0>
```



## services\.pandar\.web\.apiUrl



Rust API URL passed through APP_API_URL\.



*Type:*
string



*Default:*

```nix
"http://127.0.0.1:8080"
```



## services\.pandar\.web\.baseUrl



Public frontend URL passed through APP_BASE_URL\.



*Type:*
string



*Default:*

```nix
"http://127.0.0.1:3000"
```



## services\.pandar\.web\.extraEnvironment



Extra environment variables for pandar-web\.



*Type:*
attribute set of string



*Default:*

```nix
{ }
```



## services\.pandar\.web\.port



HTTP port for pandar-web\.



*Type:*
16 bit unsigned integer; between 0 and 65535 (both inclusive)



*Default:*

```nix
3000
```
