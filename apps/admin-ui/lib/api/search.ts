import { getFetcher, postFetcher } from "./fetcher";

export interface SearchParameterIndexingStatusRecord {
  resourceType: string;
  versionNumber: number;
  paramCount: number;
  currentHash: string;
  lastParameterChange: string;
  totalResources: number;
  indexedWithCurrent: number;
  indexedWithOld: number;
  neverIndexed: number;
  coveragePercent: number;
  indexingNeeded: boolean;
  oldestIndexedAt: string | null;
  newestIndexedAt: string | null;
}

export interface SearchIndexTableStatusRecord {
  tableName: string;
  rowCount: number;
  isUnlogged: boolean;
  sizePretty: string;
}

export interface SearchHashCollisionStatusRecord {
  tableName: string;
  collisionCount: number;
}

export interface AdminSearchParameterListItem {
  id: string;
  url: string | null;
  code: string | null;
  base: string[];
  type: string | null;
  status: string | null;
  expression: string | null;
  description: string | null;
  lastUpdated: string | null;
  serverExpectedBases: number;
  serverConfiguredBases: number;
  serverActive: boolean;
}

export interface AdminSearchParameterListResponse {
  items: AdminSearchParameterListItem[];
  total: number;
}

export const getSearchParameterIndexingStatus = async ({
  resourceType,
}: {
  resourceType?: string;
} = {}): Promise<SearchParameterIndexingStatusRecord[]> => {
  const url = resourceType
    ? `/api/admin/search-parameters/indexing-status/${encodeURIComponent(resourceType)}`
    : "/api/admin/search-parameters/indexing-status";

  return getFetcher<SearchParameterIndexingStatusRecord[]>(url);
};

export const getSearchIndexTableStatus = async (): Promise<
  SearchIndexTableStatusRecord[]
> => {
  return getFetcher<SearchIndexTableStatusRecord[]>("/api/admin/search/index-tables");
};

export const getSearchHashCollisions = async (): Promise<
  SearchHashCollisionStatusRecord[]
> => {
  return getFetcher<SearchHashCollisionStatusRecord[]>(
    "/api/admin/search/hash-collisions",
  );
};

export const getAdminSearchParameters = async ({
  q,
  status,
  type,
  resourceType,
  limit,
  offset,
}: {
  q?: string;
  status?: string;
  type?: string;
  resourceType?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<AdminSearchParameterListResponse> => {
  const params = new URLSearchParams();
  if (q) params.set("q", q);
  if (status) params.set("status", status);
  if (type) params.set("type", type);
  if (resourceType) params.set("resourceType", resourceType);
  if (typeof limit === "number") params.set("limit", String(limit));
  if (typeof offset === "number") params.set("offset", String(offset));

  const qs = params.toString();
  return getFetcher<AdminSearchParameterListResponse>(
    `/api/admin/search-parameters${qs ? `?${qs}` : ""}`
  );
};

export const toggleSearchParameterActive = async (
  id: string
): Promise<{ active: boolean }> => {
  return postFetcher<{ active: boolean }>(
    `/api/admin/search-parameters/${id}/toggle-active`
  );
};

export interface CompartmentMembershipRecord {
  compartmentType: string;
  resourceType: string;
  parameterNames: string[];
  startParam: string | null;
  endParam: string | null;
  loadedAt: string;
}

export const getCompartmentMemberships = async (): Promise<
  CompartmentMembershipRecord[]
> => {
  return getFetcher<CompartmentMembershipRecord[]>(
    "/api/admin/compartments/memberships"
  );
};
