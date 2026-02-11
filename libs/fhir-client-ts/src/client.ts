import type {
  Resource,
  Bundle,
  OperationOutcome,
  Parameters,
  CapabilityStatement,
} from "fhir/r4";

import type {
  FhirClientConfig,
  AuthConfig,
  RequestOptions,
  CreateOptions,
  UpdateOptions,
  DeleteOptions,
  ReadOptions,
  HistoryOptions,
  OperationOptions,
} from "./types/client.js";

import type {
  FhirResponse,
  ResponseMeta,
  BundleResponse,
  DeleteResponse,
  CapabilitiesResponse,
} from "./types/response.js";

import type { SearchOptions } from "./types/search.js";
import type { JsonPatchOperation } from "./types/patch.js";

import { FhirError, NetworkError, TimeoutError } from "./errors/fhir-error.js";
import { SearchBuilder } from "./search/builder.js";
import { BundleBuilder } from "./batch/builder.js";

export class FhirClient {
  private readonly baseUrl: string;
  private readonly auth?: AuthConfig;
  private readonly timeout: number;
  private readonly defaultHeaders: Record<string, string>;

  constructor(config: FhirClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, "");
    this.auth = config.auth;
    this.timeout = config.timeout ?? 30000;
    this.defaultHeaders = {
      Accept: "application/fhir+json",
      ...config.headers,
    };
  }

  private async getAuthHeader(): Promise<string | undefined> {
    if (!this.auth) return undefined;

    if ("token" in this.auth) {
      return `Bearer ${this.auth.token}`;
    }

    if ("tokenProvider" in this.auth) {
      const token = await this.auth.tokenProvider();
      return `Bearer ${token}`;
    }

    return undefined;
  }

  private async request<T>(
    method: string,
    path: string,
    options?: RequestOptions & { body?: unknown; contentType?: string }
  ): Promise<{ data: T; response: Response }> {
    const url = path.startsWith("http") ? path : `${this.baseUrl}/${path}`;

    const headers: Record<string, string> = { ...this.defaultHeaders };

    const authHeader = await this.getAuthHeader();
    if (authHeader) {
      headers["Authorization"] = authHeader;
    }

    if (options?.headers) {
      Object.assign(headers, options.headers);
    }

    if (options?.ifMatch) {
      headers["If-Match"] = options.ifMatch;
    }
    if (options?.ifNoneMatch) {
      headers["If-None-Match"] = options.ifNoneMatch;
    }
    if (options?.ifNoneExist) {
      headers["If-None-Exist"] = options.ifNoneExist;
    }
    if (options?.ifModifiedSince) {
      headers["If-Modified-Since"] = options.ifModifiedSince.toUTCString();
    }
    if (options?.prefer) {
      headers["Prefer"] = options.prefer;
    }

    let body: string | undefined;
    if (options?.body !== undefined) {
      if (options.contentType) {
        headers["Content-Type"] = options.contentType;
      } else {
        headers["Content-Type"] = "application/fhir+json";
      }
      body = JSON.stringify(options.body);
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    const signal = options?.signal
      ? this.mergeSignals(options.signal, controller.signal)
      : controller.signal;

    try {
      const response = await fetch(url, {
        method,
        headers,
        body,
        signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        let operationOutcome: OperationOutcome | undefined;
        try {
          const errorBody = await response.json();
          if (errorBody?.resourceType === "OperationOutcome") {
            operationOutcome = errorBody as OperationOutcome;
          }
        } catch {
          // Ignore JSON parse errors for error responses
        }
        throw FhirError.fromResponse(response.status, operationOutcome, response);
      }

      // Handle 204 No Content
      if (response.status === 204) {
        return { data: undefined as T, response };
      }

      const data = (await response.json()) as T;
      return { data, response };
    } catch (error) {
      clearTimeout(timeoutId);

      if (error instanceof FhirError) {
        throw error;
      }

      if (error instanceof DOMException && error.name === "AbortError") {
        throw new TimeoutError();
      }

      if (error instanceof TypeError) {
        throw new NetworkError(`Network request failed: ${error.message}`, error);
      }

      throw error;
    }
  }

  private mergeSignals(signal1: AbortSignal, signal2: AbortSignal): AbortSignal {
    const controller = new AbortController();

    const abort = () => controller.abort();

    signal1.addEventListener("abort", abort);
    signal2.addEventListener("abort", abort);

    if (signal1.aborted || signal2.aborted) {
      controller.abort();
    }

    return controller.signal;
  }

  private parseResponseMeta(response: Response): ResponseMeta {
    const meta: ResponseMeta = {
      status: response.status,
    };

    const etag = response.headers.get("ETag");
    if (etag) {
      meta.etag = etag;
    }

    const lastModified = response.headers.get("Last-Modified");
    if (lastModified) {
      meta.lastModified = new Date(lastModified);
    }

    const location = response.headers.get("Location");
    if (location) {
      meta.location = location;
    }

    const contentLocation = response.headers.get("Content-Location");
    if (contentLocation) {
      meta.contentLocation = contentLocation;
    }

    return meta;
  }

  // CRUD Operations

  async createResource<T extends Resource>(
    resource: T,
    options?: CreateOptions
  ): Promise<FhirResponse<T>> {
    const { data, response } = await this.request<T>(
      "POST",
      resource.resourceType,
      { ...options, body: resource }
    );

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async readResource<T extends Resource>(
    resourceType: string,
    id: string,
    options?: ReadOptions
  ): Promise<FhirResponse<T>> {
    let path = `${resourceType}/${id}`;

    const params = new URLSearchParams();
    if (options?.summary) {
      params.set("_summary", options.summary);
    }
    if (options?.elements?.length) {
      params.set("_elements", options.elements.join(","));
    }

    if (params.toString()) {
      path += `?${params.toString()}`;
    }

    const { data, response } = await this.request<T>("GET", path, options);

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async vreadResource<T extends Resource>(
    resourceType: string,
    id: string,
    versionId: string,
    options?: RequestOptions
  ): Promise<FhirResponse<T>> {
    const { data, response } = await this.request<T>(
      "GET",
      `${resourceType}/${id}/_history/${versionId}`,
      options
    );

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async updateResource<T extends Resource>(
    resource: T,
    options?: UpdateOptions
  ): Promise<FhirResponse<T>> {
    if (!resource.id) {
      throw new Error("Resource must have an id for update");
    }

    const { data, response } = await this.request<T>(
      "PUT",
      `${resource.resourceType}/${resource.id}`,
      { ...options, body: resource }
    );

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async patchResource<T extends Resource>(
    resourceType: string,
    id: string,
    operations: JsonPatchOperation[],
    options?: UpdateOptions
  ): Promise<FhirResponse<T>> {
    const { data, response } = await this.request<T>(
      "PATCH",
      `${resourceType}/${id}`,
      {
        ...options,
        body: operations,
        contentType: "application/json-patch+json",
      }
    );

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async deleteResource(
    resourceType: string,
    id: string,
    options?: DeleteOptions
  ): Promise<DeleteResponse> {
    const { data, response } = await this.request<OperationOutcome | undefined>(
      "DELETE",
      `${resourceType}/${id}`,
      options
    );

    return {
      operationOutcome: data,
      meta: this.parseResponseMeta(response),
    };
  }

  // Conditional Operations

  async conditionalCreate<T extends Resource>(
    resource: T,
    searchParams: string,
    options?: CreateOptions
  ): Promise<FhirResponse<T>> {
    const { data, response } = await this.request<T>(
      "POST",
      resource.resourceType,
      {
        ...options,
        body: resource,
        ifNoneExist: searchParams,
      }
    );

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async conditionalUpdate<T extends Resource>(
    resource: T,
    searchParams: string,
    options?: UpdateOptions
  ): Promise<FhirResponse<T>> {
    const { data, response } = await this.request<T>(
      "PUT",
      `${resource.resourceType}?${searchParams}`,
      { ...options, body: resource }
    );

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async conditionalDelete(
    resourceType: string,
    searchParams: string,
    options?: DeleteOptions
  ): Promise<DeleteResponse> {
    const { data, response } = await this.request<OperationOutcome | undefined>(
      "DELETE",
      `${resourceType}?${searchParams}`,
      options
    );

    return {
      operationOutcome: data,
      meta: this.parseResponseMeta(response),
    };
  }

  // Search

  search<T extends Resource>(resourceType: string): SearchBuilder<T> {
    return new SearchBuilder<T>(
      resourceType,
      (type, params, options) => this.executeSearch(type, params, options),
      (url, options) => this.executeSearchUrl(url, options)
    );
  }

  private async executeSearch(
    resourceType: string,
    params: URLSearchParams,
    options?: SearchOptions
  ): Promise<{ bundle: Bundle; meta: ResponseMeta }> {
    const query = params.toString();
    const path = query ? `${resourceType}?${query}` : resourceType;

    const { data, response } = await this.request<Bundle>("GET", path, options);

    return {
      bundle: data,
      meta: this.parseResponseMeta(response),
    };
  }

  private async executeSearchUrl(
    url: string,
    options?: SearchOptions
  ): Promise<{ bundle: Bundle; meta: ResponseMeta }> {
    const { data, response } = await this.request<Bundle>("GET", url, options);

    return {
      bundle: data,
      meta: this.parseResponseMeta(response),
    };
  }

  // Batch / Transaction

  batch(): BundleBuilder {
    return new BundleBuilder("batch", (bundle) => this.executeBundle(bundle));
  }

  transaction(): BundleBuilder {
    return new BundleBuilder("transaction", (bundle) => this.executeBundle(bundle));
  }

  private async executeBundle(
    bundle: Bundle
  ): Promise<{ bundle: Bundle; meta: ResponseMeta }> {
    const { data, response } = await this.request<Bundle>("POST", "", {
      body: bundle,
    });

    return {
      bundle: data,
      meta: this.parseResponseMeta(response),
    };
  }

  // Operations

  async operation<T extends Resource | Parameters = Parameters>(
    name: string,
    params?: Parameters,
    options?: OperationOptions
  ): Promise<FhirResponse<T>> {
    const method = options?.method ?? "POST";
    const path = `$${name}`;

    if (method === "GET" && params) {
      const searchParams = this.parametersToSearchParams(params);
      const { data, response } = await this.request<T>(
        "GET",
        `${path}?${searchParams.toString()}`,
        options
      );
      return { resource: data, meta: this.parseResponseMeta(response) };
    }

    const { data, response } = await this.request<T>(method, path, {
      ...options,
      body: params,
    });

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async typeOperation<T extends Resource | Parameters = Parameters>(
    resourceType: string,
    name: string,
    params?: Parameters,
    options?: OperationOptions
  ): Promise<FhirResponse<T>> {
    const method = options?.method ?? "POST";
    const path = `${resourceType}/$${name}`;

    if (method === "GET" && params) {
      const searchParams = this.parametersToSearchParams(params);
      const { data, response } = await this.request<T>(
        "GET",
        `${path}?${searchParams.toString()}`,
        options
      );
      return { resource: data, meta: this.parseResponseMeta(response) };
    }

    const { data, response } = await this.request<T>(method, path, {
      ...options,
      body: params,
    });

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async instanceOperation<T extends Resource | Parameters = Parameters>(
    resourceType: string,
    id: string,
    name: string,
    params?: Parameters,
    options?: OperationOptions
  ): Promise<FhirResponse<T>> {
    const method = options?.method ?? "POST";
    const path = `${resourceType}/${id}/$${name}`;

    if (method === "GET" && params) {
      const searchParams = this.parametersToSearchParams(params);
      const { data, response } = await this.request<T>(
        "GET",
        `${path}?${searchParams.toString()}`,
        options
      );
      return { resource: data, meta: this.parseResponseMeta(response) };
    }

    const { data, response } = await this.request<T>(method, path, {
      ...options,
      body: params,
    });

    return {
      resource: data,
      meta: this.parseResponseMeta(response),
    };
  }

  private parametersToSearchParams(params: Parameters): URLSearchParams {
    const searchParams = new URLSearchParams();

    if (params.parameter) {
      for (const param of params.parameter) {
        if (!param.name) continue;

        const value =
          param.valueString ??
          param.valueBoolean?.toString() ??
          param.valueInteger?.toString() ??
          param.valueDecimal?.toString() ??
          param.valueUri ??
          param.valueCode ??
          param.valueDate ??
          param.valueDateTime ??
          param.valueTime ??
          param.valueInstant;

        if (value !== undefined) {
          searchParams.append(param.name, value);
        }
      }
    }

    return searchParams;
  }

  // History

  async history(
    resourceType: string,
    id: string,
    options?: HistoryOptions
  ): Promise<BundleResponse> {
    let path = `${resourceType}/${id}/_history`;
    path = this.appendHistoryParams(path, options);

    const { data, response } = await this.request<Bundle>("GET", path, options);

    return {
      bundle: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async typeHistory(
    resourceType: string,
    options?: HistoryOptions
  ): Promise<BundleResponse> {
    let path = `${resourceType}/_history`;
    path = this.appendHistoryParams(path, options);

    const { data, response } = await this.request<Bundle>("GET", path, options);

    return {
      bundle: data,
      meta: this.parseResponseMeta(response),
    };
  }

  async systemHistory(options?: HistoryOptions): Promise<BundleResponse> {
    let path = `_history`;
    path = this.appendHistoryParams(path, options);

    const { data, response } = await this.request<Bundle>("GET", path, options);

    return {
      bundle: data,
      meta: this.parseResponseMeta(response),
    };
  }

  private appendHistoryParams(path: string, options?: HistoryOptions): string {
    if (!options) return path;

    const params = new URLSearchParams();

    if (options.count !== undefined) {
      params.set("_count", options.count.toString());
    }

    if (options.since) {
      const sinceValue =
        options.since instanceof Date
          ? options.since.toISOString()
          : options.since;
      params.set("_since", sinceValue);
    }

    if (options.at) {
      const atValue =
        options.at instanceof Date ? options.at.toISOString() : options.at;
      params.set("_at", atValue);
    }

    const queryString = params.toString();
    return queryString ? `${path}?${queryString}` : path;
  }

  // Capabilities

  async capabilities(options?: RequestOptions): Promise<CapabilitiesResponse> {
    const { data, response } = await this.request<CapabilityStatement>(
      "GET",
      "metadata",
      options
    );

    return {
      capabilityStatement: data,
      meta: this.parseResponseMeta(response),
    };
  }
}
