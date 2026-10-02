export async function request(path: string, method = "GET", body?: unknown) {
  const response = await fetch(path, {
    method,
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let result: any = {};
  try { result = JSON.parse(text); } catch { /* A complete non-JSON response has no fields. */ }
  if (!response.ok) throw new Error(result.error || "request failed");
  return result;
}
