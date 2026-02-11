import { createProxyHandlers } from "@/server/proxy";

/**
 * Proxy handler for Health endpoint
 *
 * Proxies requests from /api/health to the actual FHIR server at /health
 * This allows the admin client to check server health without exposing the FHIR server URL
 * or dealing with CORS issues.
 */
const config = {
  targetPathPrefix: "/health",
  defaultAccept: "application/json",
  errorMessage: "Failed to proxy request to health endpoint",
};

const handlers = createProxyHandlers(config);

export const GET = handlers.GET;
