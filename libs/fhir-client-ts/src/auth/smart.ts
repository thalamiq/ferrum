import type { TokenProvider } from "../types/client.js";

export interface SmartConfiguration {
  issuer: string;
  authorization_endpoint: string;
  token_endpoint: string;
  jwks_uri: string;
  capabilities?: string[];
  registration_endpoint?: string;
  introspection_endpoint?: string;
  revocation_endpoint?: string;
  userinfo_endpoint?: string;
}

export interface SmartAuthConfig {
  fhirBaseUrl: string;
  clientId: string;
  redirectUri: string;
  scope?: string;
  launch?: string;
  state?: string;
  storage?: SmartAuthStorage;
}

export interface SmartAuthStorage {
  get(key: string): string | null;
  set(key: string, value: string): void;
  remove(key: string): void;
}

interface TokenResponse {
  access_token: string;
  token_type: string;
  expires_in?: number;
  refresh_token?: string;
  scope?: string;
  patient?: string;
  encounter?: string;
  need_patient_banner?: boolean;
  smart_style_url?: string;
}

interface TokenData {
  accessToken: string;
  refreshToken?: string;
  expiresAt: number;
  patient?: string;
  scope?: string;
}

const DEFAULT_SCOPE = "openid profile fhirUser user/*.*";

/**
 * SMART on FHIR Authorization Code Flow with PKCE
 *
 * Handles OAuth2 authorization code flow with PKCE for secure authentication.
 * Supports automatic token refresh and seamless integration with FhirClient.
 */
export class SmartAuth {
  private readonly config: SmartAuthConfig;
  private readonly storage: SmartAuthStorage;
  private smartConfig: SmartConfiguration | null = null;
  private tokenData: TokenData | null = null;
  private tokenPromise: Promise<string> | null = null;

  constructor(config: SmartAuthConfig) {
    this.config = {
      scope: DEFAULT_SCOPE,
      ...config,
    };
    this.storage = config.storage ?? this.createDefaultStorage();
    this.restoreToken();
  }

  /**
   * Discover SMART configuration from the FHIR server
   */
  async discover(): Promise<SmartConfiguration> {
    if (this.smartConfig) {
      return this.smartConfig;
    }

    const url = `${this.config.fhirBaseUrl}/.well-known/smart-configuration`;
    const response = await fetch(url, {
      headers: { Accept: "application/json" },
    });

    if (!response.ok) {
      throw new Error(
        `SMART discovery failed: ${response.status} ${response.statusText}`
      );
    }

    this.smartConfig = (await response.json()) as SmartConfiguration;
    return this.smartConfig;
  }

  /**
   * Generate PKCE code verifier and challenge
   */
  private async generatePKCE(): Promise<{ verifier: string; challenge: string }> {
    if (typeof crypto === "undefined" || !crypto.getRandomValues) {
      throw new Error(
        "Web Crypto API not available. This requires a modern browser or Node.js 15+."
      );
    }

    // Generate random code verifier (43-128 characters)
    const array = new Uint8Array(32);
    crypto.getRandomValues(array);
    const verifier = btoa(String.fromCharCode(...array))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=/g, "");

    // Generate code challenge (SHA256 hash, base64url encoded)
    if (!crypto.subtle) {
      throw new Error(
        "crypto.subtle not available. This requires HTTPS in browsers or Node.js 15+."
      );
    }

    const encoder = new TextEncoder();
    const data = encoder.encode(verifier);
    const hashBuffer = await crypto.subtle.digest("SHA-256", data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const challenge = btoa(String.fromCharCode(...hashArray))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=/g, "");

    return { verifier, challenge };
  }

  /**
   * Generate random state for CSRF protection
   */
  private generateState(): string {
    const array = new Uint8Array(16);
    crypto.getRandomValues(array);
    return btoa(String.fromCharCode(...array))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=/g, "");
  }

  /**
   * Initiate authorization flow
   * In browser: redirects to authorization server
   * In Node.js: returns authorization URL
   */
  async authorize(): Promise<string> {
    const config = await this.discover();

    const { verifier, challenge } = await this.generatePKCE();
    const state = this.config.state ?? this.generateState();

    // Store PKCE verifier and state for callback verification
    this.storage.set("smart_pkce_verifier", verifier);
    this.storage.set("smart_oauth_state", state);

    const params = new URLSearchParams({
      response_type: "code",
      client_id: this.config.clientId,
      redirect_uri: this.config.redirectUri,
      scope: this.config.scope ?? DEFAULT_SCOPE,
      code_challenge: challenge,
      code_challenge_method: "S256",
      state,
      aud: this.config.fhirBaseUrl,
    });

    if (this.config.launch) {
      params.set("launch", this.config.launch);
    }

    const authUrl = `${config.authorization_endpoint}?${params.toString()}`;

    // In browser environment, redirect automatically
    if (typeof window !== "undefined" && window.location) {
      window.location.href = authUrl;
    }

    return authUrl;
  }

  /**
   * Handle authorization callback and exchange code for token
   */
  async handleCallback(
    code: string,
    state: string,
    error?: string
  ): Promise<TokenResponse> {
    if (error) {
      throw new Error(`Authorization error: ${error}`);
    }

    const storedState = this.storage.get("smart_oauth_state");
    if (!storedState || storedState !== state) {
      throw new Error("Invalid state parameter - possible CSRF attack");
    }

    const verifier = this.storage.get("smart_pkce_verifier");
    if (!verifier) {
      throw new Error("PKCE verifier not found - authorization flow may have expired");
    }

    const config = await this.discover();

    const params = new URLSearchParams({
      grant_type: "authorization_code",
      code,
      redirect_uri: this.config.redirectUri,
      client_id: this.config.clientId,
      code_verifier: verifier,
    });

    const response = await fetch(config.token_endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json",
      },
      body: params.toString(),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Token exchange failed: ${response.status} ${errorText}`);
    }

    const tokenResponse = (await response.json()) as TokenResponse;
    this.setToken(tokenResponse);

    // Clean up stored values
    this.storage.remove("smart_oauth_state");
    this.storage.remove("smart_pkce_verifier");

    return tokenResponse;
  }

  /**
   * Refresh access token using refresh token
   */
  private async refreshToken(): Promise<string> {
    if (!this.tokenData?.refreshToken) {
      throw new Error("No refresh token available");
    }

    const config = await this.discover();

    const params = new URLSearchParams({
      grant_type: "refresh_token",
      refresh_token: this.tokenData.refreshToken,
      client_id: this.config.clientId,
    });

    const response = await fetch(config.token_endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json",
      },
      body: params.toString(),
    });

    if (!response.ok) {
      const errorText = await response.text();
      // If refresh fails, clear token data
      this.clearToken();
      throw new Error(`Token refresh failed: ${response.status} ${errorText}`);
    }

    const tokenResponse = (await response.json()) as TokenResponse;
    this.setToken(tokenResponse);

    return this.tokenData.accessToken;
  }

  /**
   * Set token data and store in storage
   */
  private setToken(tokenResponse: TokenResponse): void {
    const expiresIn = tokenResponse.expires_in ?? 3600;
    const expiresAt = Date.now() + expiresIn * 1000 - 60000; // Refresh 1 minute early

    this.tokenData = {
      accessToken: tokenResponse.access_token,
      refreshToken: tokenResponse.refresh_token,
      expiresAt,
      patient: tokenResponse.patient,
      scope: tokenResponse.scope,
    };

    this.storage.set("smart_token_data", JSON.stringify(this.tokenData));
  }

  /**
   * Restore token from storage
   */
  private restoreToken(): void {
    const stored = this.storage.get("smart_token_data");
    if (stored) {
      try {
        this.tokenData = JSON.parse(stored) as TokenData;
        // Validate token hasn't expired
        if (this.tokenData && Date.now() >= this.tokenData.expiresAt) {
          this.clearToken();
        }
      } catch {
        this.clearToken();
      }
    }
  }

  /**
   * Check if user is authenticated
   */
  isAuthenticated(): boolean {
    return this.tokenData !== null && Date.now() < this.tokenData.expiresAt;
  }

  /**
   * Get current access token, refreshing if necessary
   */
  async getToken(): Promise<string> {
    if (!this.tokenData) {
      throw new Error("Not authenticated. Call authorize() first.");
    }

    // Check if token needs refresh
    if (Date.now() >= this.tokenData.expiresAt) {
      // Prevent concurrent refresh requests
      if (this.tokenPromise) {
        return this.tokenPromise;
      }

      if (this.tokenData.refreshToken) {
        this.tokenPromise = this.refreshToken().finally(() => {
          this.tokenPromise = null;
        });
        return this.tokenPromise;
      } else {
        this.clearToken();
        throw new Error("Token expired and no refresh token available");
      }
    }

    return this.tokenData.accessToken;
  }

  /**
   * Get current patient ID from token (if available)
   */
  getPatientId(): string | undefined {
    return this.tokenData?.patient;
  }

  /**
   * Get current scopes from token
   */
  getScopes(): string[] {
    if (!this.tokenData?.scope) {
      return [];
    }
    return this.tokenData.scope.split(/\s+/).filter((s) => s.length > 0);
  }

  /**
   * Clear token and logout
   */
  logout(): void {
    this.clearToken();
  }

  private clearToken(): void {
    this.tokenData = null;
    this.storage.remove("smart_token_data");
    this.storage.remove("smart_oauth_state");
    this.storage.remove("smart_pkce_verifier");
  }

  /**
   * Create token provider for FhirClient integration
   */
  tokenProvider(): TokenProvider {
    return () => this.getToken();
  }

  /**
   * Create default storage implementation
   */
  private createDefaultStorage(): SmartAuthStorage {
    // Browser environment
    if (typeof window !== "undefined" && window.sessionStorage) {
      return {
        get: (key: string) => sessionStorage.getItem(key),
        set: (key: string, value: string) => sessionStorage.setItem(key, value),
        remove: (key: string) => sessionStorage.removeItem(key),
      };
    }

    // Node.js environment - use in-memory storage
    const memoryStorage = new Map<string, string>();
    return {
      get: (key: string) => memoryStorage.get(key) ?? null,
      set: (key: string, value: string) => memoryStorage.set(key, value),
      remove: (key: string) => memoryStorage.delete(key),
    };
  }
}
