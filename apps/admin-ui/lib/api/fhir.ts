import { Bundle, Resource } from "fhir/r4";
import { getFetcher, patchFetcher, putFetcher } from "./fetcher";

export type FhirResponse = Bundle | Resource;

export const fetchFhir = async (path: string) => {
  return getFetcher<FhirResponse>(`/api/fhir/${path}`);
};

export const putFhirResource = async <T extends FhirResponse>(
  path: string,
  resource: unknown,
) => {
  return putFetcher<T>(`/api/fhir/${path}`, resource, "application/fhir+json");
};

export type JsonPatchOperation =
  | { op: "add"; path: string; value: unknown }
  | { op: "remove"; path: string }
  | { op: "replace"; path: string; value: unknown }
  | { op: "move"; from: string; path: string }
  | { op: "copy"; from: string; path: string }
  | { op: "test"; path: string; value: unknown };

export const patchFhirJsonPatch = async <T extends FhirResponse>(
  path: string,
  patch: JsonPatchOperation[],
) => {
  return patchFetcher<T>(`/api/fhir/${path}`, patch, "application/json-patch+json");
};
