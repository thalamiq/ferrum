// Main client
export { FhirClient } from "./client.js";

// Auth
export { ClientCredentialsAuth } from "./auth/index.js";
export type { ClientCredentialsConfig } from "./auth/index.js";
export { SmartAuth } from "./auth/index.js";
export type {
  SmartAuthConfig,
  SmartConfiguration,
  SmartAuthStorage,
} from "./auth/index.js";

// Builders
export { SearchBuilder } from "./search/index.js";
export { BundleBuilder } from "./batch/index.js";
export type { BundleEntryOptions, BundleResult } from "./batch/index.js";

// Errors
export {
  FhirError,
  NotFoundError,
  GoneError,
  ConflictError,
  PreconditionFailedError,
  ValidationError,
  UnprocessableEntityError,
  AuthenticationError,
  ForbiddenError,
  NetworkError,
  TimeoutError,
} from "./errors/index.js";

// Types
export type {
  // Client config
  FhirClientConfig,
  AuthConfig,
  BearerTokenAuth,
  TokenProviderAuth,
  TokenProvider,
  // Request options
  RequestOptions,
  CreateOptions,
  UpdateOptions,
  DeleteOptions,
  ReadOptions,
  HistoryOptions,
  OperationOptions,
  ResourceType,
} from "./types/client.js";

export type {
  // Response types
  ResponseMeta,
  FhirResponse,
  BundleResponse,
  DeleteResponse,
  CapabilitiesResponse,
} from "./types/response.js";

export type {
  // Search types
  SearchParams,
  SearchOptions,
  SearchResult,
  SearchResultMeta,
  SearchModifier,
  SearchPrefix,
  SearchParameter,
} from "./types/search.js";

export type {
  // Patch types
  JsonPatchOperation,
  JsonPatchAdd,
  JsonPatchRemove,
  JsonPatchReplace,
  JsonPatchMove,
  JsonPatchCopy,
  JsonPatchTest,
} from "./types/patch.js";
