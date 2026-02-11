import { createProxyHandlers } from '@/server/proxy';

/**
 * Proxy handler for Admin API endpoints
 * 
 * Proxies all requests from /api/admin/* to the actual FHIR server at /admin/*
 * This allows the admin client to make requests without exposing the FHIR server URL
 * or dealing with CORS issues.
 */
const config = {
  targetPathPrefix: '/admin',
  defaultAccept: 'application/json',
  errorMessage: 'Failed to proxy request to admin server',
};

const handlers = createProxyHandlers(config);

export const GET = handlers.GET;
export const POST = handlers.POST;
export const PUT = handlers.PUT;
export const PATCH = handlers.PATCH;
export const DELETE = handlers.DELETE;

