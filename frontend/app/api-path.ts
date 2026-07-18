const API_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/;

export function apiIdSegment(value: string, fieldName: string) {
  if (!API_ID_PATTERN.test(value)) {
    throw new Error(`${fieldName} must be a valid ID`);
  }
  return encodeURIComponent(value);
}

export function isApiId(value: string) {
  return API_ID_PATTERN.test(value);
}

export function invalidApiIdResponse() {
  return Response.json(
    { error: "invalid_id" },
    { status: 400, headers: { "cache-control": "no-store" } },
  );
}
