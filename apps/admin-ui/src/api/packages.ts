import { getFetcher, postFetcher } from "./client";

export interface PackageRecord {
  id: number;
  name: string;
  version: string;
  status: string;
  resourceCount: number;
  createdAt: string;
  errorMessage?: string;
  metadata?: Record<string, unknown>;
}

export interface PackageListResponse {
  packages: PackageRecord[];
  total: number;
  limit: number;
  offset: number;
}

export interface PackageResourceRecord {
  resourceType: string;
  resourceId: string;
  versionId: number;
  loadedAt: string;
  lastUpdated: string;
  deleted: boolean;
  resource: Record<string, unknown>;
}

export interface PackageResourcesResponse {
  packageId: number;
  resources: PackageResourceRecord[];
  total: number;
  limit: number | null;
  offset: number | null;
}

export interface InstallPackageRequest {
  name: string;
  version?: string;
  includeDependencies?: boolean;
  includeExamples?: boolean;
}

export interface InstallPackageResponse {
  accepted: boolean;
  jobId: string;
  name: string;
  version: string | null;
  includeDependencies: boolean;
  includeExamples: boolean;
}

export const getPackages = async ({
  status,
  limit,
  offset,
}: {
  status?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<PackageListResponse> => {
  const params = new URLSearchParams();
  if (status) params.set("status", status);
  if (typeof limit === "number") params.set("limit", String(limit));
  if (typeof offset === "number") params.set("offset", String(offset));

  const query = params.toString();
  const url = query ? `/admin/packages?${query}` : "/admin/packages";
  return getFetcher<PackageListResponse>(url);
};

export const getPackage = async (id: string): Promise<PackageRecord> => {
  return getFetcher<PackageRecord>(`/admin/packages/${id}`);
};

export const getPackageResources = async ({
  packageId,
  limit,
  offset,
}: {
  packageId: number;
  limit?: number;
  offset?: number;
}): Promise<PackageResourcesResponse> => {
  const params = new URLSearchParams();
  if (typeof limit === "number") params.set("limit", String(limit));
  if (typeof offset === "number") params.set("offset", String(offset));

  const query = params.toString();
  const url = query
    ? `/admin/packages/${packageId}/resources?${query}`
    : `/admin/packages/${packageId}/resources`;
  return getFetcher<PackageResourcesResponse>(url);
};

export const installPackage = async (
  request: InstallPackageRequest,
): Promise<InstallPackageResponse> => {
  return postFetcher<InstallPackageResponse>(
    "/admin/packages/install",
    request,
  );
};

// FHIR Parameters format for operations
export interface FhirParameter {
  name: string;
  valueString?: string;
  valueBoolean?: boolean;
  valueInteger?: number;
  resource?: Record<string, unknown>;
}

export interface FhirParameters {
  resourceType: "Parameters";
  parameter: FhirParameter[];
}

export interface InstallPackageOperationRequest {
  name: string;
  version?: string;
  includeExamples?: boolean;
}

export interface InstallPackageOperationResponse {
  resourceType: "Parameters";
  parameter: Array<{
    name: string;
    resource?: Record<string, unknown>;
    valueString?: string;
  }>;
}

export const installPackageOperation = async (
  request: InstallPackageOperationRequest,
): Promise<InstallPackageOperationResponse> => {
  const parameters: FhirParameters = {
    resourceType: "Parameters",
    parameter: [
      {
        name: "name",
        valueString: request.name,
      },
    ],
  };

  if (request.version) {
    parameters.parameter.push({
      name: "version",
      valueString: request.version,
    });
  }

  if (request.includeExamples !== undefined) {
    parameters.parameter.push({
      name: "includeExamples",
      valueBoolean: request.includeExamples,
    });
  }

  return postFetcher<InstallPackageOperationResponse>(
    "/fhir/$install-package",
    parameters,
  );
};
