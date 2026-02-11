import type { TokenProvider } from "../types/client.js";

export interface ClientCredentialsConfig {
  clientId: string;
  clientSecret: string;
  issuer?: string;
  tokenEndpoint?: string;
  scope?: string;
}

interface TokenResponse {
  access_token: string;
  token_type: string;
  expires_in?: number;
  scope?: string;
}

interface OidcConfig {
  token_endpoint: string;
}

export class ClientCredentialsAuth {
  private readonly config: ClientCredentialsConfig;
  private cachedToken?: string;
  private tokenExpiresAt?: number;
  private tokenEndpoint?: string;
  private tokenPromise?: Promise<string>;

  constructor(config: ClientCredentialsConfig) {
    if (!config.issuer && !config.tokenEndpoint) {
      throw new Error("Either 'issuer' or 'tokenEndpoint' must be provided");
    }
    this.config = config;
    this.tokenEndpoint = config.tokenEndpoint;
  }

  private async discoverTokenEndpoint(): Promise<string> {
    if (this.tokenEndpoint) {
      return this.tokenEndpoint;
    }

    const discoveryUrl = this.config.issuer!.replace(/\/$/, "") + "/.well-known/openid-configuration";

    const response = await fetch(discoveryUrl, {
      headers: { Accept: "application/json" },
    });

    if (!response.ok) {
      throw new Error(`OIDC discovery failed: ${response.status} ${response.statusText}`);
    }

    const config = (await response.json()) as OidcConfig;

    if (!config.token_endpoint) {
      throw new Error("Token endpoint not found in OIDC configuration");
    }

    this.tokenEndpoint = config.token_endpoint;
    return this.tokenEndpoint;
  }

  private async fetchToken(): Promise<string> {
    const tokenEndpoint = await this.discoverTokenEndpoint();

    const body = new URLSearchParams({
      grant_type: "client_credentials",
      client_id: this.config.clientId,
      client_secret: this.config.clientSecret,
    });

    if (this.config.scope) {
      body.set("scope", this.config.scope);
    }

    const response = await fetch(tokenEndpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json",
      },
      body: body.toString(),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Token request failed: ${response.status} ${errorText}`);
    }

    const tokenResponse = (await response.json()) as TokenResponse;

    this.cachedToken = tokenResponse.access_token;

    if (tokenResponse.expires_in) {
      // Refresh 60 seconds before expiry to avoid edge cases
      const bufferSeconds = 60;
      this.tokenExpiresAt = Date.now() + (tokenResponse.expires_in - bufferSeconds) * 1000;
    }

    return this.cachedToken;
  }

  private isTokenValid(): boolean {
    if (!this.cachedToken) {
      return false;
    }
    if (this.tokenExpiresAt && Date.now() >= this.tokenExpiresAt) {
      return false;
    }
    return true;
  }

  async getToken(): Promise<string> {
    if (this.isTokenValid()) {
      return this.cachedToken!;
    }

    // Prevent concurrent token requests
    if (this.tokenPromise) {
      return this.tokenPromise;
    }

    this.tokenPromise = this.fetchToken().finally(() => {
      this.tokenPromise = undefined;
    });

    return this.tokenPromise;
  }

  tokenProvider(): TokenProvider {
    return () => this.getToken();
  }

  clearCache(): void {
    this.cachedToken = undefined;
    this.tokenExpiresAt = undefined;
  }
}
