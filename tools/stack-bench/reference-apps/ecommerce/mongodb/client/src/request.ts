export async function request(path: string, token: string | null, options: RequestInit = {}) {
  const response = await fetch(path, { ...options, headers: {
    "Content-Type": "application/json", ...(token ? { Authorization: `Bearer ${token}` } : {}),
  } });
  const text = await response.text();
  let data: any = {};
  try { data = JSON.parse(text); } catch { /* A complete non-JSON response has no fields. */ }
  if (!response.ok) throw new Error(data.error || "Request failed");
  return data;
}
