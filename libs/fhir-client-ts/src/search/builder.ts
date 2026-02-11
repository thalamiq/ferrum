import type { Bundle, Resource, BundleEntry } from "fhir/r4";
import type {
  SearchResult,
  SearchResultMeta,
  SearchModifier,
  SearchOptions,
} from "../types/search.js";
import type { ResponseMeta } from "../types/response.js";

type ExecuteFn = (
  resourceType: string,
  params: URLSearchParams,
  options?: SearchOptions
) => Promise<{ bundle: Bundle; meta: ResponseMeta }>;

type ExecuteUrlFn = (
  url: string,
  options?: SearchOptions
) => Promise<{ bundle: Bundle; meta: ResponseMeta }>;

export class SearchBuilder<T extends Resource = Resource> {
  private readonly resourceType: string;
  private readonly params: URLSearchParams;
  private readonly executeFn: ExecuteFn;
  private readonly executeUrlFn: ExecuteUrlFn;
  private searchOptions?: SearchOptions;

  constructor(
    resourceType: string,
    executeFn: ExecuteFn,
    executeUrlFn: ExecuteUrlFn
  ) {
    this.resourceType = resourceType;
    this.params = new URLSearchParams();
    this.executeFn = executeFn;
    this.executeUrlFn = executeUrlFn;
  }

  where(name: string, value: string | string[]): this {
    if (Array.isArray(value)) {
      // Multiple values for same parameter = OR
      this.params.append(name, value.join(","));
    } else {
      this.params.append(name, value);
    }
    return this;
  }

  whereExact(name: string, value: string): this {
    this.params.append(`${name}:exact`, value);
    return this;
  }

  whereContains(name: string, value: string): this {
    this.params.append(`${name}:contains`, value);
    return this;
  }

  whereText(name: string, value: string): this {
    this.params.append(`${name}:text`, value);
    return this;
  }

  whereMissing(name: string, isMissing: boolean = true): this {
    this.params.append(`${name}:missing`, String(isMissing));
    return this;
  }

  whereNot(name: string, value: string): this {
    this.params.append(`${name}:not`, value);
    return this;
  }

  whereBelow(name: string, value: string): this {
    this.params.append(`${name}:below`, value);
    return this;
  }

  whereAbove(name: string, value: string): this {
    this.params.append(`${name}:above`, value);
    return this;
  }

  whereIn(name: string, valueSetUrl: string): this {
    this.params.append(`${name}:in`, valueSetUrl);
    return this;
  }

  whereNotIn(name: string, valueSetUrl: string): this {
    this.params.append(`${name}:not-in`, valueSetUrl);
    return this;
  }

  whereOfType(name: string, system: string, code: string): this {
    this.params.append(`${name}:of-type`, `${system}|${code}`);
    return this;
  }

  whereIdentifier(name: string, system: string, value: string): this {
    this.params.append(`${name}:identifier`, `${system}|${value}`);
    return this;
  }

  withModifier(name: string, modifier: SearchModifier, value: string): this {
    this.params.append(`${name}:${modifier}`, value);
    return this;
  }

  include(value: string): this {
    this.params.append("_include", value);
    return this;
  }

  includeIterate(value: string): this {
    this.params.append("_include:iterate", value);
    return this;
  }

  revinclude(value: string): this {
    this.params.append("_revinclude", value);
    return this;
  }

  revincludeIterate(value: string): this {
    this.params.append("_revinclude:iterate", value);
    return this;
  }

  sort(field: string): this {
    const existing = this.params.get("_sort");
    if (existing) {
      this.params.set("_sort", `${existing},${field}`);
    } else {
      this.params.set("_sort", field);
    }
    return this;
  }

  count(n: number): this {
    this.params.set("_count", String(n));
    return this;
  }

  offset(n: number): this {
    this.params.set("_offset", String(n));
    return this;
  }

  summary(mode: "true" | "text" | "data" | "count" | "false"): this {
    this.params.set("_summary", mode);
    return this;
  }

  elements(...elements: string[]): this {
    this.params.set("_elements", elements.join(","));
    return this;
  }

  contained(mode: "true" | "false" | "both"): this {
    this.params.set("_contained", mode);
    return this;
  }

  containedType(mode: "container" | "contained"): this {
    this.params.set("_containedType", mode);
    return this;
  }

  total(mode: "none" | "estimate" | "accurate"): this {
    this.params.set("_total", mode);
    return this;
  }

  withOptions(options: SearchOptions): this {
    this.searchOptions = options;
    return this;
  }

  getParams(): URLSearchParams {
    return new URLSearchParams(this.params);
  }

  async execute(): Promise<SearchResult<T>> {
    const { bundle, meta } = await this.executeFn(
      this.resourceType,
      this.params,
      this.searchOptions
    );

    return this.createSearchResult(bundle, meta);
  }

  private createSearchResult(bundle: Bundle, meta: ResponseMeta): SearchResult<T> {
    const resources = this.extractResources(bundle);
    const searchMeta: SearchResultMeta = {
      ...meta,
      total: bundle.total,
      link: bundle.link,
    };

    const getLink = (rel: string): string | undefined => {
      return bundle.link?.find((l) => l.relation === rel)?.url;
    };

    const result: SearchResult<T> = {
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
      },
    };

    return result;
  }

  private extractResources(bundle: Bundle): T[] {
    if (!bundle.entry) {
      return [];
    }

    return bundle.entry
      .filter((entry: BundleEntry): entry is BundleEntry & { resource: T } => {
        if (!entry.resource) return false;
        // Only include resources matching the search type (exclude _include results)
        const searchMode = entry.search?.mode;
        return searchMode === undefined || searchMode === "match";
      })
      .map((entry) => entry.resource);
  }
}
