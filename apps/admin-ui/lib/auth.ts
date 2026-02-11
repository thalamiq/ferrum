const AUTH_PROXY_PATH = "/api/admin/ui/auth";
const SESSION_PROXY_PATH = "/api/admin/ui/session";
const LOGOUT_PROXY_PATH = "/api/admin/ui/logout";

/**
 * Authentication response from server
 */
export interface AuthResponse {
  authenticated: boolean;
  token?: string;
}

/**
 * Authenticate with the admin password
 */
export async function authenticate(password: string): Promise<AuthResponse> {
  const response = await fetch(AUTH_PROXY_PATH, {
    method: "POST",
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ password }),
  });

  if (!response.ok) {
    if (response.status === 401) {
      return { authenticated: false };
    }
    throw new Error(`Authentication failed: ${response.statusText}`);
  }

  const data: AuthResponse = await response.json();

  return data;
}

/**
 * Check if a valid admin session exists.
 */
export async function hasSession(): Promise<boolean> {
  const response = await fetch(SESSION_PROXY_PATH, {
    method: "GET",
    credentials: "include",
  });
  return response.ok;
}

/**
 * Clear authentication (logout)
 */
export async function logout(): Promise<void> {
  await fetch(LOGOUT_PROXY_PATH, {
    method: "POST",
    credentials: "include",
  });
}
