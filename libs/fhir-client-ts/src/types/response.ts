import type { Resource, Bundle, OperationOutcome, CapabilityStatement } from "fhir/r4";

export interface ResponseMeta {
  status: number;
  etag?: string;
  lastModified?: Date;
  location?: string;
  contentLocation?: string;
}

export interface FhirResponse<T extends Resource = Resource> {
  resource: T;
  meta: ResponseMeta;
}

export interface BundleResponse {
  bundle: Bundle;
  meta: ResponseMeta;
}

export interface DeleteResponse {
  operationOutcome?: OperationOutcome;
  meta: ResponseMeta;
}

export interface CapabilitiesResponse {
  capabilityStatement: CapabilityStatement;
  meta: ResponseMeta;
}
