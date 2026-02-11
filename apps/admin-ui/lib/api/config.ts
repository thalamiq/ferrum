/**
 * Runtime configuration API client
 */

import { getFetcher, postFetcher, putFetcher } from "./fetcher";

export interface RuntimeConfigEntry {
  key: string;
  value: unknown;
  default_value: unknown;
  category: string;
  description: string;
  value_type: "boolean" | "integer" | "string" | "string_enum";
  updated_at: string | null;
  updated_by: string | null;
  is_default: boolean;
  enum_values?: string[];
  min_value?: number;
  max_value?: number;
}

export interface RuntimeConfigListResponse {
  entries: RuntimeConfigEntry[];
  total: number;
}

export interface RuntimeConfigAuditEntry {
  id: number;
  key: string;
  old_value: unknown | null;
  new_value: unknown;
  changed_by: string | null;
  changed_at: string;
  change_type: "create" | "update" | "delete" | "reset";
}

export interface RuntimeConfigAuditResponse {
  entries: RuntimeConfigAuditEntry[];
  total: number;
}

export interface UpdateConfigRequest {
  value: unknown;
  updated_by?: string;
}

/**
 * Fetch all runtime configuration entries
 */
export async function fetchRuntimeConfig(
  category?: string
): Promise<RuntimeConfigListResponse> {
  const params = new URLSearchParams();
  if (category) {
    params.append("category", category);
  }
  const queryString = params.toString();
  const url = `/api/admin/config${queryString ? `?${queryString}` : ""}`;
  return getFetcher<RuntimeConfigListResponse>(url);
}

/**
 * Fetch a single runtime configuration entry
 */
export async function fetchRuntimeConfigEntry(
  key: string
): Promise<RuntimeConfigEntry> {
  return getFetcher<RuntimeConfigEntry>(`/api/admin/config/${encodeURIComponent(key)}`);
}

/**
 * Update a runtime configuration value
 */
export async function updateRuntimeConfig(
  key: string,
  request: UpdateConfigRequest
): Promise<RuntimeConfigEntry> {
  return putFetcher<RuntimeConfigEntry>(
    `/api/admin/config/${encodeURIComponent(key)}`,
    request
  );
}

/**
 * Reset a runtime configuration value to its default
 */
export async function resetRuntimeConfig(
  key: string
): Promise<RuntimeConfigEntry> {
  return postFetcher<RuntimeConfigEntry>(
    `/api/admin/config/${encodeURIComponent(key)}/reset`
  );
}

/**
 * Fetch runtime configuration audit log
 */
export async function fetchRuntimeConfigAudit(options?: {
  key?: string;
  limit?: number;
  offset?: number;
}): Promise<RuntimeConfigAuditResponse> {
  const params = new URLSearchParams();
  if (options?.key) {
    params.append("key", options.key);
  }
  if (options?.limit !== undefined) {
    params.append("limit", options.limit.toString());
  }
  if (options?.offset !== undefined) {
    params.append("offset", options.offset.toString());
  }
  const queryString = params.toString();
  const url = `/api/admin/config/audit${queryString ? `?${queryString}` : ""}`;
  return getFetcher<RuntimeConfigAuditResponse>(url);
}

/**
 * Configuration categories for organizing settings
 */
export const CONFIG_CATEGORIES = [
  { id: "logging", label: "Logging" },
  { id: "search", label: "Search" },
  { id: "interactions", label: "Interactions" },
  { id: "format", label: "Format" },
  { id: "behavior", label: "Behavior" },
  { id: "audit", label: "Audit" },
] as const;

export type ConfigCategory = (typeof CONFIG_CATEGORIES)[number]["id"];
