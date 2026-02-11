import { NextRequest } from "next/server";
import { createProxyHandlers, proxyRequest } from "@/server/proxy";

/**
 * Proxy handler for FHIR API endpoints
 *
 * Proxies all requests from /api/fhir/* to the actual FHIR server at /fhir/*
 * This allows the admin client to make requests without exposing the FHIR server URL
 * or dealing with CORS issues.
 */
const config = {
  targetPathPrefix: "/fhir",
  defaultAccept: "application/fhir+json",
  errorMessage: "Failed to proxy request to FHIR server",
  forwardLocation: true,
};

const handlers = createProxyHandlers(config);

export const GET = handlers.GET;
export const POST = handlers.POST;
export const PUT = handlers.PUT;
export const PATCH = handlers.PATCH;
export const DELETE = handlers.DELETE;

export async function HEAD(
  request: NextRequest,
  { params }: { params: Promise<{ path?: string[] }> }
) {
  return proxyRequest(request, params, "HEAD", config);
}
