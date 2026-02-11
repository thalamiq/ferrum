'use strict';

// src/errors/fhir-error.ts
var FhirError = class _FhirError extends Error {
  status;
  operationOutcome;
  response;
  constructor(message, status, operationOutcome, response) {
    super(message);
    this.name = "FhirError";
    this.status = status;
    this.operationOutcome = operationOutcome;
    this.response = response;
    Object.setPrototypeOf(this, new.target.prototype);
  }
  get issues() {
    return this.operationOutcome?.issue ?? [];
  }
  static fromResponse(status, operationOutcome, response) {
    const message = operationOutcome?.issue?.[0]?.diagnostics ?? operationOutcome?.issue?.[0]?.details?.text ?? `FHIR request failed with status ${status}`;
    switch (status) {
      case 400:
        return new ValidationError(message, operationOutcome, response);
      case 401:
        return new AuthenticationError(message, operationOutcome, response);
      case 403:
        return new ForbiddenError(message, operationOutcome, response);
      case 404:
        return new NotFoundError(message, operationOutcome, response);
      case 409:
        return new ConflictError(message, operationOutcome, response);
      case 410:
        return new GoneError(message, operationOutcome, response);
      case 412:
        return new PreconditionFailedError(message, operationOutcome, response);
      case 422:
        return new UnprocessableEntityError(message, operationOutcome, response);
      default:
        return new _FhirError(message, status, operationOutcome, response);
    }
  }
};
var NotFoundError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 404, operationOutcome, response);
    this.name = "NotFoundError";
  }
};
var GoneError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 410, operationOutcome, response);
    this.name = "GoneError";
  }
};
var ConflictError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 409, operationOutcome, response);
    this.name = "ConflictError";
  }
};
var PreconditionFailedError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 412, operationOutcome, response);
    this.name = "PreconditionFailedError";
  }
};
var ValidationError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 400, operationOutcome, response);
    this.name = "ValidationError";
  }
};
var UnprocessableEntityError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 422, operationOutcome, response);
    this.name = "UnprocessableEntityError";
  }
};
var AuthenticationError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 401, operationOutcome, response);
    this.name = "AuthenticationError";
  }
};
var ForbiddenError = class extends FhirError {
  constructor(message, operationOutcome, response) {
    super(message, 403, operationOutcome, response);
    this.name = "ForbiddenError";
  }
};
var NetworkError = class extends Error {
  cause;
  constructor(message, cause) {
    super(message);
    this.name = "NetworkError";
    this.cause = cause;
    Object.setPrototypeOf(this, new.target.prototype);
  }
};
var TimeoutError = class extends Error {
  constructor(message = "Request timed out") {
    super(message);
    this.name = "TimeoutError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
};

// src/search/builder.ts
var SearchBuilder = class {
  resourceType;
  params;
  executeFn;
  executeUrlFn;
  searchOptions;
  constructor(resourceType, executeFn, executeUrlFn) {
    this.resourceType = resourceType;
    this.params = new URLSearchParams();
    this.executeFn = executeFn;
    this.executeUrlFn = executeUrlFn;
  }
  where(name, value) {
    if (Array.isArray(value)) {
      this.params.append(name, value.join(","));
    } else {
      this.params.append(name, value);
    }
    return this;
  }
  whereExact(name, value) {
    this.params.append(`${name}:exact`, value);
    return this;
  }
  whereContains(name, value) {
    this.params.append(`${name}:contains`, value);
    return this;
  }
  whereText(name, value) {
    this.params.append(`${name}:text`, value);
    return this;
  }
  whereMissing(name, isMissing = true) {
    this.params.append(`${name}:missing`, String(isMissing));
    return this;
  }
  whereNot(name, value) {
    this.params.append(`${name}:not`, value);
    return this;
  }
  whereBelow(name, value) {
    this.params.append(`${name}:below`, value);
    return this;
  }
  whereAbove(name, value) {
    this.params.append(`${name}:above`, value);
    return this;
  }
  whereIn(name, valueSetUrl) {
    this.params.append(`${name}:in`, valueSetUrl);
    return this;
  }
  whereNotIn(name, valueSetUrl) {
    this.params.append(`${name}:not-in`, valueSetUrl);
    return this;
  }
  whereOfType(name, system, code) {
    this.params.append(`${name}:of-type`, `${system}|${code}`);
    return this;
  }
  whereIdentifier(name, system, value) {
    this.params.append(`${name}:identifier`, `${system}|${value}`);
    return this;
  }
  withModifier(name, modifier, value) {
    this.params.append(`${name}:${modifier}`, value);
    return this;
  }
  include(value) {
    this.params.append("_include", value);
    return this;
  }
  includeIterate(value) {
    this.params.append("_include:iterate", value);
    return this;
  }
  revinclude(value) {
    this.params.append("_revinclude", value);
    return this;
  }
  revincludeIterate(value) {
    this.params.append("_revinclude:iterate", value);
    return this;
  }
  sort(field) {
    const existing = this.params.get("_sort");
    if (existing) {
      this.params.set("_sort", `${existing},${field}`);
    } else {
      this.params.set("_sort", field);
    }
    return this;
  }
  count(n) {
    this.params.set("_count", String(n));
    return this;
  }
  offset(n) {
    this.params.set("_offset", String(n));
    return this;
  }
  summary(mode) {
    this.params.set("_summary", mode);
    return this;
  }
  elements(...elements) {
    this.params.set("_elements", elements.join(","));
    return this;
  }
  contained(mode) {
    this.params.set("_contained", mode);
    return this;
  }
  containedType(mode) {
    this.params.set("_containedType", mode);
    return this;
  }
  total(mode) {
    this.params.set("_total", mode);
    return this;
  }
  withOptions(options) {
    this.searchOptions = options;
    return this;
  }
  getParams() {
    return new URLSearchParams(this.params);
  }
  async execute() {
    const { bundle, meta } = await this.executeFn(
      this.resourceType,
      this.params,
      this.searchOptions
    );
    return this.createSearchResult(bundle, meta);
  }
  createSearchResult(bundle, meta) {
    const resources = this.extractResources(bundle);
    const searchMeta = {
      ...meta,
      total: bundle.total,
      link: bundle.link
    };
    const getLink = (rel) => {
      return bundle.link?.find((l) => l.relation === rel)?.url;
    };
    const result = {
      bundle,
      resources,
      total: bundle.total,
      meta: searchMeta,
      hasNextPage: () => !!getLink("next"),
      hasPrevPage: () => !!getLink("previous") || !!getLink("prev"),
      nextPage: async () => {
        const nextUrl = getLink("next");
        if (!nextUrl) {
          throw new Error("No next page available");
        }
        const { bundle: nextBundle, meta: nextMeta } = await this.executeUrlFn(
          nextUrl,
          this.searchOptions
        );
        return this.createSearchResult(nextBundle, nextMeta);
      },
      prevPage: async () => {
        const prevUrl = getLink("previous") ?? getLink("prev");
        if (!prevUrl) {
          throw new Error("No previous page available");
        }
        const { bundle: prevBundle, meta: prevMeta } = await this.executeUrlFn(
          prevUrl,
          this.searchOptions
        );
        return this.createSearchResult(prevBundle, prevMeta);
      }
    };
    return result;
  }
  extractResources(bundle) {
    if (!bundle.entry) {
      return [];
    }
    return bundle.entry.filter((entry) => {
      if (!entry.resource) return false;
      const searchMode = entry.search?.mode;
      return searchMode === void 0 || searchMode === "match";
    }).map((entry) => entry.resource);
  }
};

// src/batch/builder.ts
var BundleBuilder = class {
  type;
  entries = [];
  executeFn;
  constructor(type, executeFn) {
    this.type = type;
    this.executeFn = executeFn;
  }
  create(resource, options) {
    const entry = {
      fullUrl: options?.fullUrl,
      resource,
      request: {
        method: "POST",
        url: resource.resourceType,
        ifNoneExist: options?.ifNoneExist
      }
    };
    this.entries.push(entry);
    return this;
  }
  update(resource, options) {
    if (!resource.id) {
      throw new Error("Resource must have an id for update");
    }
    const entry = {
      fullUrl: options?.fullUrl ?? `${resource.resourceType}/${resource.id}`,
      resource,
      request: {
        method: "PUT",
        url: `${resource.resourceType}/${resource.id}`,
        ifMatch: options?.ifMatch
      }
    };
    this.entries.push(entry);
    return this;
  }
  conditionalUpdate(resource, searchParams, options) {
    const entry = {
      fullUrl: options?.fullUrl,
      resource,
      request: {
        method: "PUT",
        url: `${resource.resourceType}?${searchParams}`,
        ifMatch: options?.ifMatch
      }
    };
    this.entries.push(entry);
    return this;
  }
  patch(resourceType, id, operations, options) {
    const entry = {
      fullUrl: options?.fullUrl ?? `${resourceType}/${id}`,
      resource: {
        resourceType: "Binary",
        contentType: "application/json-patch+json",
        data: btoa(JSON.stringify(operations))
      },
      request: {
        method: "PATCH",
        url: `${resourceType}/${id}`,
        ifMatch: options?.ifMatch
      }
    };
    this.entries.push(entry);
    return this;
  }
  delete(resourceType, id, options) {
    const entry = {
      fullUrl: options?.fullUrl ?? `${resourceType}/${id}`,
      request: {
        method: "DELETE",
        url: `${resourceType}/${id}`,
        ifMatch: options?.ifMatch
      }
    };
    this.entries.push(entry);
    return this;
  }
  conditionalDelete(resourceType, searchParams, options) {
    const entry = {
      fullUrl: options?.fullUrl,
      request: {
        method: "DELETE",
        url: `${resourceType}?${searchParams}`
      }
    };
    this.entries.push(entry);
    return this;
  }
  read(resourceType, id, options) {
    const entry = {
      fullUrl: options?.fullUrl ?? `${resourceType}/${id}`,
      request: {
        method: "GET",
        url: `${resourceType}/${id}`,
        ifNoneMatch: options?.ifNoneMatch,
        ifModifiedSince: options?.ifModifiedSince
      }
    };
    this.entries.push(entry);
    return this;
  }
  search(resourceType, params, options) {
    const entry = {
      fullUrl: options?.fullUrl,
      request: {
        method: "GET",
        url: `${resourceType}?${params}`
      }
    };
    this.entries.push(entry);
    return this;
  }
  addEntry(entry) {
    this.entries.push(entry);
    return this;
  }
  getBundle() {
    return {
      resourceType: "Bundle",
      type: this.type,
      entry: this.entries
    };
  }
  async execute() {
    const bundle = this.getBundle();
    const { bundle: responseBundle, meta } = await this.executeFn(bundle);
    return this.createBundleResult(responseBundle, meta);
  }
  createBundleResult(bundle, meta) {
    const fullUrlToIndex = /* @__PURE__ */ new Map();
    this.entries.forEach((entry, index) => {
      if (entry.fullUrl) {
        fullUrlToIndex.set(entry.fullUrl, index);
      }
    });
    return {
      bundle,
      meta,
      getResource(index) {
        return bundle.entry?.[index]?.resource;
      },
      getResourceByFullUrl(fullUrl) {
        const index = fullUrlToIndex.get(fullUrl);
        if (index === void 0) return void 0;
        return bundle.entry?.[index]?.resource;
      },
      isSuccess(index) {
        const status = this.getStatus(index);
        return status !== void 0 && status >= 200 && status < 300;
      },
      getStatus(index) {
        const statusStr = bundle.entry?.[index]?.response?.status;
        if (!statusStr) return void 0;
        const match = statusStr.match(/^(\d+)/);
        return match?.[1] ? parseInt(match[1], 10) : void 0;
      },
      getLocation(index) {
        return bundle.entry?.[index]?.response?.location;
      }
    };
  }
};

// src/client.ts
var FhirClient = class {
  baseUrl;
  auth;
  timeout;
  defaultHeaders;
  constructor(config) {
    this.baseUrl = config.baseUrl.replace(/\/$/, "");
    this.auth = config.auth;
    this.timeout = config.timeout ?? 3e4;
    this.defaultHeaders = {
      Accept: "application/fhir+json",
      ...config.headers
    };
  }
  async getAuthHeader() {
    if (!this.auth) return void 0;
    if ("token" in this.auth) {
      return `Bearer ${this.auth.token}`;
    }
    if ("tokenProvider" in this.auth) {
      const token = await this.auth.tokenProvider();
      return `Bearer ${token}`;
    }
    return void 0;
  }
  async request(method, path, options) {
    const url = path.startsWith("http") ? path : `${this.baseUrl}/${path}`;
    const headers = { ...this.defaultHeaders };
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
    let body;
    if (options?.body !== void 0) {
      if (options.contentType) {
        headers["Content-Type"] = options.contentType;
      } else {
        headers["Content-Type"] = "application/fhir+json";
      }
      body = JSON.stringify(options.body);
    }
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);
    const signal = options?.signal ? this.mergeSignals(options.signal, controller.signal) : controller.signal;
    try {
      const response = await fetch(url, {
        method,
        headers,
        body,
        signal
      });
      clearTimeout(timeoutId);
      if (!response.ok) {
        let operationOutcome;
        try {
          const errorBody = await response.json();
          if (errorBody?.resourceType === "OperationOutcome") {
            operationOutcome = errorBody;
          }
        } catch {
        }
        throw FhirError.fromResponse(response.status, operationOutcome, response);
      }
      if (response.status === 204) {
        return { data: void 0, response };
      }
      const data = await response.json();
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
  mergeSignals(signal1, signal2) {
    const controller = new AbortController();
    const abort = () => controller.abort();
    signal1.addEventListener("abort", abort);
    signal2.addEventListener("abort", abort);
    if (signal1.aborted || signal2.aborted) {
      controller.abort();
    }
    return controller.signal;
  }
  parseResponseMeta(response) {
    const meta = {
      status: response.status
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
  async createResource(resource, options) {
    const { data, response } = await this.request(
      "POST",
      resource.resourceType,
      { ...options, body: resource }
    );
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async readResource(resourceType, id, options) {
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
    const { data, response } = await this.request("GET", path, options);
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async vreadResource(resourceType, id, versionId, options) {
    const { data, response } = await this.request(
      "GET",
      `${resourceType}/${id}/_history/${versionId}`,
      options
    );
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async updateResource(resource, options) {
    if (!resource.id) {
      throw new Error("Resource must have an id for update");
    }
    const { data, response } = await this.request(
      "PUT",
      `${resource.resourceType}/${resource.id}`,
      { ...options, body: resource }
    );
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async patchResource(resourceType, id, operations, options) {
    const { data, response } = await this.request(
      "PATCH",
      `${resourceType}/${id}`,
      {
        ...options,
        body: operations,
        contentType: "application/json-patch+json"
      }
    );
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async deleteResource(resourceType, id, options) {
    const { data, response } = await this.request(
      "DELETE",
      `${resourceType}/${id}`,
      options
    );
    return {
      operationOutcome: data,
      meta: this.parseResponseMeta(response)
    };
  }
  // Conditional Operations
  async conditionalCreate(resource, searchParams, options) {
    const { data, response } = await this.request(
      "POST",
      resource.resourceType,
      {
        ...options,
        body: resource,
        ifNoneExist: searchParams
      }
    );
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async conditionalUpdate(resource, searchParams, options) {
    const { data, response } = await this.request(
      "PUT",
      `${resource.resourceType}?${searchParams}`,
      { ...options, body: resource }
    );
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async conditionalDelete(resourceType, searchParams, options) {
    const { data, response } = await this.request(
      "DELETE",
      `${resourceType}?${searchParams}`,
      options
    );
    return {
      operationOutcome: data,
      meta: this.parseResponseMeta(response)
    };
  }
  // Search
  search(resourceType) {
    return new SearchBuilder(
      resourceType,
      (type, params, options) => this.executeSearch(type, params, options),
      (url, options) => this.executeSearchUrl(url, options)
    );
  }
  async executeSearch(resourceType, params, options) {
    const query = params.toString();
    const path = query ? `${resourceType}?${query}` : resourceType;
    const { data, response } = await this.request("GET", path, options);
    return {
      bundle: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async executeSearchUrl(url, options) {
    const { data, response } = await this.request("GET", url, options);
    return {
      bundle: data,
      meta: this.parseResponseMeta(response)
    };
  }
  // Batch / Transaction
  batch() {
    return new BundleBuilder("batch", (bundle) => this.executeBundle(bundle));
  }
  transaction() {
    return new BundleBuilder("transaction", (bundle) => this.executeBundle(bundle));
  }
  async executeBundle(bundle) {
    const { data, response } = await this.request("POST", "", {
      body: bundle
    });
    return {
      bundle: data,
      meta: this.parseResponseMeta(response)
    };
  }
  // Operations
  async operation(name, params, options) {
    const method = options?.method ?? "POST";
    const path = `$${name}`;
    if (method === "GET" && params) {
      const searchParams = this.parametersToSearchParams(params);
      const { data: data2, response: response2 } = await this.request(
        "GET",
        `${path}?${searchParams.toString()}`,
        options
      );
      return { resource: data2, meta: this.parseResponseMeta(response2) };
    }
    const { data, response } = await this.request(method, path, {
      ...options,
      body: params
    });
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async typeOperation(resourceType, name, params, options) {
    const method = options?.method ?? "POST";
    const path = `${resourceType}/$${name}`;
    if (method === "GET" && params) {
      const searchParams = this.parametersToSearchParams(params);
      const { data: data2, response: response2 } = await this.request(
        "GET",
        `${path}?${searchParams.toString()}`,
        options
      );
      return { resource: data2, meta: this.parseResponseMeta(response2) };
    }
    const { data, response } = await this.request(method, path, {
      ...options,
      body: params
    });
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async instanceOperation(resourceType, id, name, params, options) {
    const method = options?.method ?? "POST";
    const path = `${resourceType}/${id}/$${name}`;
    if (method === "GET" && params) {
      const searchParams = this.parametersToSearchParams(params);
      const { data: data2, response: response2 } = await this.request(
        "GET",
        `${path}?${searchParams.toString()}`,
        options
      );
      return { resource: data2, meta: this.parseResponseMeta(response2) };
    }
    const { data, response } = await this.request(method, path, {
      ...options,
      body: params
    });
    return {
      resource: data,
      meta: this.parseResponseMeta(response)
    };
  }
  parametersToSearchParams(params) {
    const searchParams = new URLSearchParams();
    if (params.parameter) {
      for (const param of params.parameter) {
        if (!param.name) continue;
        const value = param.valueString ?? param.valueBoolean?.toString() ?? param.valueInteger?.toString() ?? param.valueDecimal?.toString() ?? param.valueUri ?? param.valueCode ?? param.valueDate ?? param.valueDateTime ?? param.valueTime ?? param.valueInstant;
        if (value !== void 0) {
          searchParams.append(param.name, value);
        }
      }
    }
    return searchParams;
  }
  // History
  async history(resourceType, id, options) {
    let path = `${resourceType}/${id}/_history`;
    path = this.appendHistoryParams(path, options);
    const { data, response } = await this.request("GET", path, options);
    return {
      bundle: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async typeHistory(resourceType, options) {
    let path = `${resourceType}/_history`;
    path = this.appendHistoryParams(path, options);
    const { data, response } = await this.request("GET", path, options);
    return {
      bundle: data,
      meta: this.parseResponseMeta(response)
    };
  }
  async systemHistory(options) {
    let path = `_history`;
    path = this.appendHistoryParams(path, options);
    const { data, response } = await this.request("GET", path, options);
    return {
      bundle: data,
      meta: this.parseResponseMeta(response)
    };
  }
  appendHistoryParams(path, options) {
    if (!options) return path;
    const params = new URLSearchParams();
    if (options.count !== void 0) {
      params.set("_count", options.count.toString());
    }
    if (options.since) {
      const sinceValue = options.since instanceof Date ? options.since.toISOString() : options.since;
      params.set("_since", sinceValue);
    }
    if (options.at) {
      const atValue = options.at instanceof Date ? options.at.toISOString() : options.at;
      params.set("_at", atValue);
    }
    const queryString = params.toString();
    return queryString ? `${path}?${queryString}` : path;
  }
  // Capabilities
  async capabilities(options) {
    const { data, response } = await this.request(
      "GET",
      "metadata",
      options
    );
    return {
      capabilityStatement: data,
      meta: this.parseResponseMeta(response)
    };
  }
};

// src/auth/client-credentials.ts
var ClientCredentialsAuth = class {
  config;
  cachedToken;
  tokenExpiresAt;
  tokenEndpoint;
  tokenPromise;
  constructor(config) {
    if (!config.issuer && !config.tokenEndpoint) {
      throw new Error("Either 'issuer' or 'tokenEndpoint' must be provided");
    }
    this.config = config;
    this.tokenEndpoint = config.tokenEndpoint;
  }
  async discoverTokenEndpoint() {
    if (this.tokenEndpoint) {
      return this.tokenEndpoint;
    }
    const discoveryUrl = this.config.issuer.replace(/\/$/, "") + "/.well-known/openid-configuration";
    const response = await fetch(discoveryUrl, {
      headers: { Accept: "application/json" }
    });
    if (!response.ok) {
      throw new Error(`OIDC discovery failed: ${response.status} ${response.statusText}`);
    }
    const config = await response.json();
    if (!config.token_endpoint) {
      throw new Error("Token endpoint not found in OIDC configuration");
    }
    this.tokenEndpoint = config.token_endpoint;
    return this.tokenEndpoint;
  }
  async fetchToken() {
    const tokenEndpoint = await this.discoverTokenEndpoint();
    const body = new URLSearchParams({
      grant_type: "client_credentials",
      client_id: this.config.clientId,
      client_secret: this.config.clientSecret
    });
    if (this.config.scope) {
      body.set("scope", this.config.scope);
    }
    const response = await fetch(tokenEndpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json"
      },
      body: body.toString()
    });
    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Token request failed: ${response.status} ${errorText}`);
    }
    const tokenResponse = await response.json();
    this.cachedToken = tokenResponse.access_token;
    if (tokenResponse.expires_in) {
      const bufferSeconds = 60;
      this.tokenExpiresAt = Date.now() + (tokenResponse.expires_in - bufferSeconds) * 1e3;
    }
    return this.cachedToken;
  }
  isTokenValid() {
    if (!this.cachedToken) {
      return false;
    }
    if (this.tokenExpiresAt && Date.now() >= this.tokenExpiresAt) {
      return false;
    }
    return true;
  }
  async getToken() {
    if (this.isTokenValid()) {
      return this.cachedToken;
    }
    if (this.tokenPromise) {
      return this.tokenPromise;
    }
    this.tokenPromise = this.fetchToken().finally(() => {
      this.tokenPromise = void 0;
    });
    return this.tokenPromise;
  }
  tokenProvider() {
    return () => this.getToken();
  }
  clearCache() {
    this.cachedToken = void 0;
    this.tokenExpiresAt = void 0;
  }
};

// src/auth/smart.ts
var DEFAULT_SCOPE = "openid profile fhirUser user/*.*";
var SmartAuth = class {
  config;
  storage;
  smartConfig = null;
  tokenData = null;
  tokenPromise = null;
  constructor(config) {
    this.config = {
      scope: DEFAULT_SCOPE,
      ...config
    };
    this.storage = config.storage ?? this.createDefaultStorage();
    this.restoreToken();
  }
  /**
   * Discover SMART configuration from the FHIR server
   */
  async discover() {
    if (this.smartConfig) {
      return this.smartConfig;
    }
    const url = `${this.config.fhirBaseUrl}/.well-known/smart-configuration`;
    const response = await fetch(url, {
      headers: { Accept: "application/json" }
    });
    if (!response.ok) {
      throw new Error(
        `SMART discovery failed: ${response.status} ${response.statusText}`
      );
    }
    this.smartConfig = await response.json();
    return this.smartConfig;
  }
  /**
   * Generate PKCE code verifier and challenge
   */
  async generatePKCE() {
    if (typeof crypto === "undefined" || !crypto.getRandomValues) {
      throw new Error(
        "Web Crypto API not available. This requires a modern browser or Node.js 15+."
      );
    }
    const array = new Uint8Array(32);
    crypto.getRandomValues(array);
    const verifier = btoa(String.fromCharCode(...array)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
    if (!crypto.subtle) {
      throw new Error(
        "crypto.subtle not available. This requires HTTPS in browsers or Node.js 15+."
      );
    }
    const encoder = new TextEncoder();
    const data = encoder.encode(verifier);
    const hashBuffer = await crypto.subtle.digest("SHA-256", data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const challenge = btoa(String.fromCharCode(...hashArray)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
    return { verifier, challenge };
  }
  /**
   * Generate random state for CSRF protection
   */
  generateState() {
    const array = new Uint8Array(16);
    crypto.getRandomValues(array);
    return btoa(String.fromCharCode(...array)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
  }
  /**
   * Initiate authorization flow
   * In browser: redirects to authorization server
   * In Node.js: returns authorization URL
   */
  async authorize() {
    const config = await this.discover();
    const { verifier, challenge } = await this.generatePKCE();
    const state = this.config.state ?? this.generateState();
    this.storage.set("smart_pkce_verifier", verifier);
    this.storage.set("smart_oauth_state", state);
    const params = new URLSearchParams({
      response_type: "code",
      client_id: this.config.clientId,
      redirect_uri: this.config.redirectUri,
      scope: this.config.scope ?? DEFAULT_SCOPE,
      code_challenge: challenge,
      code_challenge_method: "S256",
      state,
      aud: this.config.fhirBaseUrl
    });
    if (this.config.launch) {
      params.set("launch", this.config.launch);
    }
    const authUrl = `${config.authorization_endpoint}?${params.toString()}`;
    if (typeof window !== "undefined" && window.location) {
      window.location.href = authUrl;
    }
    return authUrl;
  }
  /**
   * Handle authorization callback and exchange code for token
   */
  async handleCallback(code, state, error) {
    if (error) {
      throw new Error(`Authorization error: ${error}`);
    }
    const storedState = this.storage.get("smart_oauth_state");
    if (!storedState || storedState !== state) {
      throw new Error("Invalid state parameter - possible CSRF attack");
    }
    const verifier = this.storage.get("smart_pkce_verifier");
    if (!verifier) {
      throw new Error("PKCE verifier not found - authorization flow may have expired");
    }
    const config = await this.discover();
    const params = new URLSearchParams({
      grant_type: "authorization_code",
      code,
      redirect_uri: this.config.redirectUri,
      client_id: this.config.clientId,
      code_verifier: verifier
    });
    const response = await fetch(config.token_endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json"
      },
      body: params.toString()
    });
    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Token exchange failed: ${response.status} ${errorText}`);
    }
    const tokenResponse = await response.json();
    this.setToken(tokenResponse);
    this.storage.remove("smart_oauth_state");
    this.storage.remove("smart_pkce_verifier");
    return tokenResponse;
  }
  /**
   * Refresh access token using refresh token
   */
  async refreshToken() {
    if (!this.tokenData?.refreshToken) {
      throw new Error("No refresh token available");
    }
    const config = await this.discover();
    const params = new URLSearchParams({
      grant_type: "refresh_token",
      refresh_token: this.tokenData.refreshToken,
      client_id: this.config.clientId
    });
    const response = await fetch(config.token_endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json"
      },
      body: params.toString()
    });
    if (!response.ok) {
      const errorText = await response.text();
      this.clearToken();
      throw new Error(`Token refresh failed: ${response.status} ${errorText}`);
    }
    const tokenResponse = await response.json();
    this.setToken(tokenResponse);
    return this.tokenData.accessToken;
  }
  /**
   * Set token data and store in storage
   */
  setToken(tokenResponse) {
    const expiresIn = tokenResponse.expires_in ?? 3600;
    const expiresAt = Date.now() + expiresIn * 1e3 - 6e4;
    this.tokenData = {
      accessToken: tokenResponse.access_token,
      refreshToken: tokenResponse.refresh_token,
      expiresAt,
      patient: tokenResponse.patient,
      scope: tokenResponse.scope
    };
    this.storage.set("smart_token_data", JSON.stringify(this.tokenData));
  }
  /**
   * Restore token from storage
   */
  restoreToken() {
    const stored = this.storage.get("smart_token_data");
    if (stored) {
      try {
        this.tokenData = JSON.parse(stored);
        if (this.tokenData && Date.now() >= this.tokenData.expiresAt) {
          this.clearToken();
        }
      } catch {
        this.clearToken();
      }
    }
  }
  /**
   * Check if user is authenticated
   */
  isAuthenticated() {
    return this.tokenData !== null && Date.now() < this.tokenData.expiresAt;
  }
  /**
   * Get current access token, refreshing if necessary
   */
  async getToken() {
    if (!this.tokenData) {
      throw new Error("Not authenticated. Call authorize() first.");
    }
    if (Date.now() >= this.tokenData.expiresAt) {
      if (this.tokenPromise) {
        return this.tokenPromise;
      }
      if (this.tokenData.refreshToken) {
        this.tokenPromise = this.refreshToken().finally(() => {
          this.tokenPromise = null;
        });
        return this.tokenPromise;
      } else {
        this.clearToken();
        throw new Error("Token expired and no refresh token available");
      }
    }
    return this.tokenData.accessToken;
  }
  /**
   * Get current patient ID from token (if available)
   */
  getPatientId() {
    return this.tokenData?.patient;
  }
  /**
   * Get current scopes from token
   */
  getScopes() {
    if (!this.tokenData?.scope) {
      return [];
    }
    return this.tokenData.scope.split(/\s+/).filter((s) => s.length > 0);
  }
  /**
   * Clear token and logout
   */
  logout() {
    this.clearToken();
  }
  clearToken() {
    this.tokenData = null;
    this.storage.remove("smart_token_data");
    this.storage.remove("smart_oauth_state");
    this.storage.remove("smart_pkce_verifier");
  }
  /**
   * Create token provider for FhirClient integration
   */
  tokenProvider() {
    return () => this.getToken();
  }
  /**
   * Create default storage implementation
   */
  createDefaultStorage() {
    if (typeof window !== "undefined" && window.sessionStorage) {
      return {
        get: (key) => sessionStorage.getItem(key),
        set: (key, value) => sessionStorage.setItem(key, value),
        remove: (key) => sessionStorage.removeItem(key)
      };
    }
    const memoryStorage = /* @__PURE__ */ new Map();
    return {
      get: (key) => memoryStorage.get(key) ?? null,
      set: (key, value) => memoryStorage.set(key, value),
      remove: (key) => memoryStorage.delete(key)
    };
  }
};

exports.AuthenticationError = AuthenticationError;
exports.BundleBuilder = BundleBuilder;
exports.ClientCredentialsAuth = ClientCredentialsAuth;
exports.ConflictError = ConflictError;
exports.FhirClient = FhirClient;
exports.FhirError = FhirError;
exports.ForbiddenError = ForbiddenError;
exports.GoneError = GoneError;
exports.NetworkError = NetworkError;
exports.NotFoundError = NotFoundError;
exports.PreconditionFailedError = PreconditionFailedError;
exports.SearchBuilder = SearchBuilder;
exports.SmartAuth = SmartAuth;
exports.TimeoutError = TimeoutError;
exports.UnprocessableEntityError = UnprocessableEntityError;
exports.ValidationError = ValidationError;
//# sourceMappingURL=index.cjs.map
//# sourceMappingURL=index.cjs.map