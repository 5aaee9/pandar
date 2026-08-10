const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

type ReadinessResult = {
  externalAuthEnabled: boolean;
  error: string | null;
};

type ReadinessResponse = {
  checks?: {
    external_auth?: {
      ready?: boolean;
      detail?: string;
    };
  };
};

export async function fetchExternalAuthStatus(): Promise<ReadinessResult> {
  try {
    const response = await fetch(`${apiUrl}/readyz`, { cache: "no-store" });
    if (!response.ok) {
      return {
        externalAuthEnabled: false,
        error: `Readiness check returned ${response.status}`,
      };
    }
    const body = (await response.json()) as ReadinessResponse;
    const externalAuth = body.checks?.external_auth;
    return {
      externalAuthEnabled:
        externalAuth?.ready === true && externalAuth.detail !== "disabled",
      error: null,
    };
  } catch (error) {
    return {
      externalAuthEnabled: false,
      error: `Readiness check failed: ${error instanceof Error ? error.message : "unknown error"}`,
    };
  }
}
