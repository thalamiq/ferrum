import { Bundle, Resource } from "fhir/r4";
import { deleteFetcher, getFetcher, patchFetcher, putFetcher } from "./client";

export type FhirResponse = Bundle | Resource;

export const fetchFhir = async (path: string) => {
  return getFetcher<FhirResponse>(`/fhir/${path}`);
};

export const putFhirResource = async <T extends FhirResponse>(
  path: string,
  resource: unknown,
) => {
  return putFetcher<T>(`/fhir/${path}`, resource, "application/fhir+json");
};

export const deleteFhirResource = async (resourceType: string, id: string) => {
  return deleteFetcher(`/fhir/${resourceType}/${id}`);
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
  return patchFetcher<T>(
    `/fhir/${path}`,
    patch,
    "application/json-patch+json",
  );
};
