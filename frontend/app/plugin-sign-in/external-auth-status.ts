import { decodeHubResponse } from "../hub-contract";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

type ReadinessResult = {
  externalAuthEnabled: boolean;
  error: string | null;
};

export async function fetchExternalAuthStatus(): Promise<ReadinessResult> {
  try {
    const response = await fetch(`${apiUrl}/api/v1/auth/status`, {
      cache: "no-store",
    });
    if (!response.ok) {
      return {
        externalAuthEnabled: false,
        error: `Auth status check returned ${response.status}`,
      };
    }
    const body = decodeHubResponse("AuthStatusResponse", await response.json());
    const externalAuth = body.external_auth;
    return {
      externalAuthEnabled:
        externalAuth?.enabled === true && externalAuth.ready === true,
      error:
        externalAuth?.enabled === true && externalAuth.ready !== true
          ? "External authentication is not ready"
          : null,
    };
  } catch (error) {
    return {
      externalAuthEnabled: false,
      error: `Readiness check failed: ${error instanceof Error ? error.message : "unknown error"}`,
    };
  }
}
