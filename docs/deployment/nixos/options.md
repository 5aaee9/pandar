# NixOS Module Options

Generated from `nixosModules.default`.

## services\.pandar\.enable

Whether to enable Pandar hub and web services\.

_Type:_
boolean

_Default:_

```nix
false
```

_Example:_

```nix
true
```

## services\.pandar\.agent\.enable

Whether to run pandar-agent when Pandar is enabled\.

_Type:_
boolean

_Default:_

```nix
false
```

## services\.pandar\.agent\.package

pandar-agent package to run\.

_Type:_
package

_Default:_

```nix
<derivation pandar-agent-0.1.0>
```

## services\.pandar\.agent\.agentId

Agent ID passed through PANDAR_AGENT_ID\.

_Type:_
null or string

_Default:_

```nix
null
```

## services\.pandar\.agent\.artifactRoot

Artifact root passed through PANDAR_ARTIFACT_ROOT\.

_Type:_
absolute path

_Default:_

```nix
"/var/lib/pandar-agent/artifacts"
```

## services\.pandar\.agent\.credential

Agent credential passed through PANDAR_AGENT_CREDENTIAL\.

_Type:_
null or string

_Default:_

```nix
null
```

## services\.pandar\.agent\.environmentFile

Optional systemd EnvironmentFile for agent secrets\.

_Type:_
null or absolute path

_Default:_

```nix
null
```

## services\.pandar\.agent\.extraEnvironment

Extra environment variables for pandar-agent\.

_Type:_
attribute set of string

_Default:_

```nix
{ }
```

## services\.pandar\.agent\.hubGrpcUrl

Hub gRPC URL passed through PANDAR_HUB_GRPC_URL\.

_Type:_
string

_Default:_

```nix
"http://127.0.0.1:50051"
```

## services\.pandar\.agent\.name

Agent name passed through PANDAR_AGENT_NAME\.

_Type:_
string

_Default:_

```nix
"local-agent"
```

## services\.pandar\.agent\.printers

Printer endpoint JSON passed through PANDAR_PRINTERS\.

_Type:_
string

_Default:_

```nix
"[]"
```

## services\.pandar\.agent\.tenantId

Tenant ID passed through PANDAR_TENANT_ID\.

_Type:_
null or string

_Default:_

```nix
null
```

## services\.pandar\.hub\.enable

Whether to run pandar-hub when Pandar is enabled\.

_Type:_
boolean

_Default:_

```nix
true
```

## services\.pandar\.hub\.package

pandar-hub package to run\.

_Type:_
package

_Default:_

```nix
<derivation pandar-hub-0.1.0>
```

## services\.pandar\.hub\.bind

HTTP bind address for pandar-hub\.

_Type:_
string

_Default:_

```nix
"0.0.0.0:8080"
```

## services\.pandar\.hub\.controlPlane

Hub control plane passed through PANDAR_CONTROL_PLANE\.

_Type:_
one of “in-process”, “nats”

_Default:_

```nix
"in-process"
```

## services\.pandar\.hub\.databaseUrl

Database URL passed through PANDAR_DATABASE_URL\.

_Type:_
string

_Default:_

```nix
"sqlite:///var/lib/pandar-hub/pandar.db"
```

## services\.pandar\.hub\.extraEnvironment

Extra environment variables for pandar-hub\.

_Type:_
attribute set of string

_Default:_

```nix
{ }
```

## services\.pandar\.hub\.grpcBind

gRPC bind address for pandar-hub agent connections\.

_Type:_
string

_Default:_

```nix
"0.0.0.0:50051"
```

## services\.pandar\.hub\.nats\.mode

NATS source for the hub control plane\. `external` uses `services.pandar.hub.nats.url`;
`service` enables the local NixOS NATS service and points the hub at it\.

_Type:_
one of “external”, “service”

_Default:_

```nix
"external"
```

## services\.pandar\.hub\.nats\.subject

Optional NATS subject passed through PANDAR_NATS_SUBJECT\.

_Type:_
null or string

_Default:_

```nix
null
```

## services\.pandar\.hub\.nats\.url

External NATS URL passed through PANDAR_NATS_URL when the hub uses the NATS control plane\.

_Type:_
null or string

_Default:_

```nix
null
```

## services\.pandar\.hub\.spoolDir

Artifact spool directory passed through PANDAR_SPOOL_DIR\.

_Type:_
absolute path

_Default:_

```nix
"/var/lib/pandar-hub/spool"
```

## services\.pandar\.web\.enable

Whether to run pandar-web when Pandar is enabled\.

_Type:_
boolean

_Default:_

```nix
true
```

## services\.pandar\.web\.package

pandar-web package to run\.

_Type:_
package

_Default:_

```nix
<derivation pandar-web-0.1.0>
```

## services\.pandar\.web\.apiUrl

Rust API URL passed through APP_API_URL\.

_Type:_
string

_Default:_

```nix
"http://127.0.0.1:8080"
```

## services\.pandar\.web\.baseUrl

Public frontend URL passed through APP_BASE_URL\.

_Type:_
string

_Default:_

```nix
"http://127.0.0.1:3000"
```

## services\.pandar\.web\.extraEnvironment

Extra environment variables for pandar-web\.

_Type:_
attribute set of string

_Default:_

```nix
{ }
```

## services\.pandar\.web\.port

HTTP port for pandar-web\.

_Type:_
16 bit unsigned integer; between 0 and 65535 (both inclusive)

_Default:_

```nix
3000
```

## services\.pandar-auth\.enable

Whether to enable self-hosted Pandar Better Auth issuer\.

_Type:_
boolean

_Default:_

```nix
false
```

_Example:_

```nix
true
```

## services\.pandar-auth\.package

pandar-auth package to run\.

_Type:_
package

_Default:_

```nix
<derivation pandar-auth-0.1.0>
```

## services\.pandar-auth\.baseURL

Public Better Auth URL passed through PANDAR_AUTH_BASE_URL\.

_Type:_
string

_Default:_

```nix
"http://127.0.0.1:3001"
```

## services\.pandar-auth\.bind

HTTP bind address for pandar-auth, formatted as host:port\.

_Type:_
string

_Default:_

```nix
"127.0.0.1:3001"
```

## services\.pandar-auth\.dashboardCallbackUrl

Dashboard callback URL passed through PANDAR_AUTH_DASHBOARD_CALLBACK_URL\.

_Type:_
string

_Default:_

```nix
"http://127.0.0.1:3000/auth/betterauth/callback"
```

## services\.pandar-auth\.dashboardSignOutUrl

Dashboard sign-out URL passed through PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL\.

_Type:_
string

_Default:_

```nix
"http://127.0.0.1:3000/auth/betterauth/session"
```

## services\.pandar-auth\.databaseFile

SQLite database file passed through PANDAR_AUTH_DATABASE_FILE\.

_Type:_
absolute path

_Default:_

```nix
"/var/lib/pandar-auth/auth.db"
```

## services\.pandar-auth\.email\.brandName

Brand name used in magic-link email copy, passed through PANDAR_AUTH_EMAIL_BRAND_NAME\.

_Type:_
string

_Default:_

```nix
"Pandar"
```

## services\.pandar-auth\.email\.from

From address passed through PANDAR_AUTH_EMAIL_FROM\.

_Type:_
string

## services\.pandar-auth\.email\.magicLinkTtlSeconds

Email magic-link expiration in seconds passed through PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS\.

_Type:_
positive integer, meaning >0

_Default:_

```nix
1800
```

## services\.pandar-auth\.email\.provider

Email delivery provider passed through PANDAR_AUTH_EMAIL_PROVIDER\.

_Type:_
one of “resend”, “smtp”

_Default:_

```nix
"resend"
```

## services\.pandar-auth\.email\.smtp\.host

SMTP host passed through PANDAR_AUTH_SMTP_HOST when email\.provider is smtp\.

_Type:_
string

_Default:_

```nix
""
```

## services\.pandar-auth\.email\.smtp\.port

SMTP port passed through PANDAR_AUTH_SMTP_PORT when email\.provider is smtp\.

_Type:_
positive integer, meaning >0

_Default:_

```nix
587
```

## services\.pandar-auth\.email\.smtp\.tls

SMTP TLS mode passed through PANDAR_AUTH_SMTP_TLS when email\.provider is smtp\.

_Type:_
one of “starttls”, “tls”, “none”

_Default:_

```nix
"starttls"
```

## services\.pandar-auth\.email\.smtp\.username

SMTP username passed through PANDAR_AUTH_SMTP_USERNAME when email\.provider is smtp\.

_Type:_
string

_Default:_

```nix
""
```

## services\.pandar-auth\.environmentFile

Optional systemd EnvironmentFile for Better Auth secrets such as BETTER_AUTH_SECRET, RESEND_API_KEY, or PANDAR_AUTH_SMTP_PASSWORD\.

_Type:_
null or absolute path

_Default:_

```nix
null
```

## services\.pandar-auth\.extraEnvironment

Extra environment variables for pandar-auth\.

_Type:_
attribute set of string

_Default:_

```nix
{ }
```

## services\.pandar-auth\.jwtMaxAgeSeconds

Better Auth JWT expiration in seconds passed through PANDAR_AUTH_JWT_MAX_AGE_SECONDS\.

_Type:_
positive integer, meaning >0

_Default:_

```nix
43200
```

## services\.pandar-auth\.trustedOrigins

Trusted dashboard origins passed through PANDAR_AUTH_TRUSTED_ORIGINS\.

_Type:_
list of string

_Default:_

```nix
[
  "http://127.0.0.1:3000"
]
```
