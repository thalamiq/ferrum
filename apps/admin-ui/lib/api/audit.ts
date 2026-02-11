import { getFetcher } from "./fetcher";

export interface AuditEventListItem {
  id: number;
  timestamp: string;
  action: string;
  httpMethod: string;
  fhirAction: string;
  resourceType: string | null;
  resourceId: string | null;
  patientId: string | null;
  clientId: string | null;
  userId: string | null;
  requestId: string | null;
  statusCode: number;
  outcome: string;
  auditEvent: unknown;
  details: unknown | null;
}

export interface AuditEventDetail {
  id: number;
  timestamp: string;
  action: string;
  httpMethod: string;
  fhirAction: string;
  resourceType: string | null;
  resourceId: string | null;
  versionId: number | null;
  patientId: string | null;
  clientId: string | null;
  userId: string | null;
  scopes: string[];
  tokenType: string;
  clientIp: string | null;
  userAgent: string | null;
  requestId: string | null;
  statusCode: number;
  outcome: string;
  auditEvent: unknown;
  details: unknown | null;
}

export interface AuditEventListResponse {
  items: AuditEventListItem[];
  total: number;
}

export interface ListAuditEventsParams {
  action?: string;
  outcome?: string;
  resourceType?: string;
  resourceId?: string;
  patientId?: string;
  clientId?: string;
  userId?: string;
  requestId?: string;
  limit?: number;
  offset?: number;
}

export const listAuditEvents = async (
  params: ListAuditEventsParams = {}
): Promise<AuditEventListResponse> => {
  const urlParams = new URLSearchParams();

  if (params.action) {
    urlParams.set("action", params.action);
  }
  if (params.outcome) {
    urlParams.set("outcome", params.outcome);
  }
  if (params.resourceType) {
    urlParams.set("resourceType", params.resourceType);
  }
  if (params.resourceId) {
    urlParams.set("resourceId", params.resourceId);
  }
  if (params.patientId) {
    urlParams.set("patientId", params.patientId);
  }
  if (params.clientId) {
    urlParams.set("clientId", params.clientId);
  }
  if (params.userId) {
    urlParams.set("userId", params.userId);
  }
  if (params.requestId) {
    urlParams.set("requestId", params.requestId);
  }
  if (typeof params.limit === "number") {
    urlParams.set("limit", String(params.limit));
  }
  if (typeof params.offset === "number") {
    urlParams.set("offset", String(params.offset));
  }

  const query = urlParams.toString();
  const url = query ? `/api/admin/audit/events?${query}` : "/api/admin/audit/events";
  return getFetcher<AuditEventListResponse>(url);
};

export const getAuditEvent = async (id: number): Promise<AuditEventDetail> => {
  return getFetcher<AuditEventDetail>(`/api/admin/audit/events/${id}`);
};
