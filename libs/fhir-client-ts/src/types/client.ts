import type { Resource } from "fhir/r4";

export type TokenProvider = () => Promise<string> | string;

export interface BearerTokenAuth {
  token: string;
}

export interface TokenProviderAuth {
  tokenProvider: TokenProvider;
}

export type AuthConfig = BearerTokenAuth | TokenProviderAuth;

export interface FhirClientConfig {
  baseUrl: string;
  auth?: AuthConfig;
  timeout?: number;
  headers?: Record<string, string>;
}

export interface RequestOptions {
  headers?: Record<string, string>;
  signal?: AbortSignal;
  ifMatch?: string;
  ifNoneMatch?: string;
  ifNoneExist?: string;
  ifModifiedSince?: Date;
  prefer?: "return=minimal" | "return=representation" | "return=OperationOutcome";
}

export interface CreateOptions extends RequestOptions {
  prefer?: "return=minimal" | "return=representation" | "return=OperationOutcome";
}

export interface UpdateOptions extends RequestOptions {
  ifMatch?: string;
}

export interface DeleteOptions extends RequestOptions {
  ifMatch?: string;
}

export interface ReadOptions extends RequestOptions {
  summary?: "true" | "text" | "data" | "count" | "false";
  elements?: string[];
}

export interface HistoryOptions extends RequestOptions {
  count?: number;
  since?: Date | string;
  at?: Date | string;
}

export interface OperationOptions extends RequestOptions {
  method?: "GET" | "POST";
}

export function isBearerTokenAuth(auth: AuthConfig): auth is BearerTokenAuth {
  return "token" in auth;
}

export function isTokenProviderAuth(auth: AuthConfig): auth is TokenProviderAuth {
  return "tokenProvider" in auth;
}

export type ResourceType = Resource["resourceType"];
