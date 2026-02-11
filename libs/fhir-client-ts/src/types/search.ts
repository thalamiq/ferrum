import type { Bundle, Resource } from "fhir/r4";
import type { ResponseMeta } from "./response.js";

export interface SearchParams {
  [key: string]: string | string[] | undefined;
}

export interface SearchOptions {
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

export interface SearchResultMeta extends ResponseMeta {
  total?: number;
  link?: Bundle["link"];
}

export interface SearchResult<T extends Resource = Resource> {
  bundle: Bundle;
  resources: T[];
  total?: number;
  meta: SearchResultMeta;
  hasNextPage(): boolean;
  hasPrevPage(): boolean;
  nextPage(): Promise<SearchResult<T>>;
  prevPage(): Promise<SearchResult<T>>;
}

export type SearchModifier =
  | "exact"
  | "contains"
  | "text"
  | "in"
  | "not-in"
  | "below"
  | "above"
  | "of-type"
  | "missing"
  | "not"
  | "identifier";

export type SearchPrefix = "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "sa" | "eb" | "ap";

export interface SearchParameter {
  name: string;
  value: string;
  modifier?: SearchModifier;
}
