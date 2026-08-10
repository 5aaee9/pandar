const apiUrl =
  process.env.APP_PUBLIC_API_URL ??
  process.env.APP_API_URL ??
  "http://localhost:8080";

export async function GET() {
  return Response.json(
    { hubUrl: apiUrl.trim().replace(/\/+$/, "") },
    {
      headers: {
        "Access-Control-Allow-Origin": "*",
        "Cache-Control": "no-store",
      },
    },
  );
}
