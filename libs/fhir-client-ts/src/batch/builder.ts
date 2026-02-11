import type { Bundle, BundleEntry, Resource, FhirResource } from "fhir/r4";
import type { ResponseMeta } from "../types/response.js";
import type { JsonPatchOperation } from "../types/patch.js";

type BundleType = "batch" | "transaction";

export interface BundleEntryOptions {
  fullUrl?: string;
  ifMatch?: string;
  ifNoneMatch?: string;
  ifNoneExist?: string;
  ifModifiedSince?: string;
}

export interface BundleResult {
  bundle: Bundle;
  meta: ResponseMeta;
  getResource<T extends Resource>(index: number): T | undefined;
  getResourceByFullUrl<T extends Resource>(fullUrl: string): T | undefined;
  isSuccess(index: number): boolean;
  getStatus(index: number): number | undefined;
  getLocation(index: number): string | undefined;
}

type ExecuteFn = (
  bundle: Bundle
) => Promise<{ bundle: Bundle; meta: ResponseMeta }>;

export class BundleBuilder {
  private readonly type: BundleType;
  private readonly entries: BundleEntry[] = [];
  private readonly executeFn: ExecuteFn;

  constructor(type: BundleType, executeFn: ExecuteFn) {
    this.type = type;
    this.executeFn = executeFn;
  }

  create<T extends Resource>(resource: T, options?: BundleEntryOptions): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl,
      resource: resource as FhirResource,
      request: {
        method: "POST",
        url: resource.resourceType,
        ifNoneExist: options?.ifNoneExist,
      },
    };
    this.entries.push(entry);
    return this;
  }

  update<T extends Resource>(resource: T, options?: BundleEntryOptions): this {
    if (!resource.id) {
      throw new Error("Resource must have an id for update");
    }

    const entry: BundleEntry = {
      fullUrl: options?.fullUrl ?? `${resource.resourceType}/${resource.id}`,
      resource: resource as FhirResource,
      request: {
        method: "PUT",
        url: `${resource.resourceType}/${resource.id}`,
        ifMatch: options?.ifMatch,
      },
    };
    this.entries.push(entry);
    return this;
  }

  conditionalUpdate<T extends Resource>(
    resource: T,
    searchParams: string,
    options?: BundleEntryOptions
  ): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl,
      resource: resource as FhirResource,
      request: {
        method: "PUT",
        url: `${resource.resourceType}?${searchParams}`,
        ifMatch: options?.ifMatch,
      },
    };
    this.entries.push(entry);
    return this;
  }

  patch(
    resourceType: string,
    id: string,
    operations: JsonPatchOperation[],
    options?: BundleEntryOptions
  ): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl ?? `${resourceType}/${id}`,
      resource: {
        resourceType: "Binary",
        contentType: "application/json-patch+json",
        data: btoa(JSON.stringify(operations)),
      },
      request: {
        method: "PATCH",
        url: `${resourceType}/${id}`,
        ifMatch: options?.ifMatch,
      },
    };
    this.entries.push(entry);
    return this;
  }

  delete(
    resourceType: string,
    id: string,
    options?: BundleEntryOptions
  ): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl ?? `${resourceType}/${id}`,
      request: {
        method: "DELETE",
        url: `${resourceType}/${id}`,
        ifMatch: options?.ifMatch,
      },
    };
    this.entries.push(entry);
    return this;
  }

  conditionalDelete(
    resourceType: string,
    searchParams: string,
    options?: BundleEntryOptions
  ): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl,
      request: {
        method: "DELETE",
        url: `${resourceType}?${searchParams}`,
      },
    };
    this.entries.push(entry);
    return this;
  }

  read(resourceType: string, id: string, options?: BundleEntryOptions): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl ?? `${resourceType}/${id}`,
      request: {
        method: "GET",
        url: `${resourceType}/${id}`,
        ifNoneMatch: options?.ifNoneMatch,
        ifModifiedSince: options?.ifModifiedSince,
      },
    };
    this.entries.push(entry);
    return this;
  }

  search(resourceType: string, params: string, options?: BundleEntryOptions): this {
    const entry: BundleEntry = {
      fullUrl: options?.fullUrl,
      request: {
        method: "GET",
        url: `${resourceType}?${params}`,
      },
    };
    this.entries.push(entry);
    return this;
  }

  addEntry(entry: BundleEntry): this {
    this.entries.push(entry);
    return this;
  }

  getBundle(): Bundle {
    return {
      resourceType: "Bundle",
      type: this.type,
      entry: this.entries,
    };
  }

  async execute(): Promise<BundleResult> {
    const bundle = this.getBundle();
    const { bundle: responseBundle, meta } = await this.executeFn(bundle);

    return this.createBundleResult(responseBundle, meta);
  }

  private createBundleResult(bundle: Bundle, meta: ResponseMeta): BundleResult {
    const fullUrlToIndex = new Map<string, number>();
    this.entries.forEach((entry, index) => {
      if (entry.fullUrl) {
        fullUrlToIndex.set(entry.fullUrl, index);
      }
    });

    return {
      bundle,
      meta,

      getResource<T extends Resource>(index: number): T | undefined {
        return bundle.entry?.[index]?.resource as T | undefined;
      },

      getResourceByFullUrl<T extends Resource>(fullUrl: string): T | undefined {
        const index = fullUrlToIndex.get(fullUrl);
        if (index === undefined) return undefined;
        return bundle.entry?.[index]?.resource as T | undefined;
      },

      isSuccess(index: number): boolean {
        const status = this.getStatus(index);
        return status !== undefined && status >= 200 && status < 300;
      },

      getStatus(index: number): number | undefined {
        const statusStr = bundle.entry?.[index]?.response?.status;
        if (!statusStr) return undefined;
        const match = statusStr.match(/^(\d+)/);
        return match?.[1] ? parseInt(match[1], 10) : undefined;
      },

      getLocation(index: number): string | undefined {
        return bundle.entry?.[index]?.response?.location;
      },
    };
  }
}
