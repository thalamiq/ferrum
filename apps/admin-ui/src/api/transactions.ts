import { getFetcher } from "./client";

export interface TransactionListItem {
  id: string;
  type: string;
  status: string;
  entryCount: number | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  errorMessage: string | null;
}

export interface TransactionEntryItem {
  entryIndex: number;
  method: string;
  url: string;
  status: number | null;
  resourceType: string | null;
  resourceId: string | null;
  versionId: number | null;
  errorMessage: string | null;
}

export interface TransactionDetail extends TransactionListItem {
  entries: TransactionEntryItem[];
}

export interface TransactionListResponse {
  items: TransactionListItem[];
  total: number;
}

export interface ListTransactionsParams {
  bundleType?: string;
  status?: string;
  limit?: number;
  offset?: number;
}

export const listTransactions = async (
  params: ListTransactionsParams = {},
): Promise<TransactionListResponse> => {
  const urlParams = new URLSearchParams();

  if (params.bundleType) urlParams.set("bundleType", params.bundleType);
  if (params.status) urlParams.set("status", params.status);
  if (typeof params.limit === "number")
    urlParams.set("limit", String(params.limit));
  if (typeof params.offset === "number")
    urlParams.set("offset", String(params.offset));

  const query = urlParams.toString();
  const url = query
    ? `/admin/transactions?${query}`
    : "/admin/transactions";
  return getFetcher<TransactionListResponse>(url);
};

export const getTransaction = async (
  id: string,
): Promise<TransactionDetail> => {
  return getFetcher<TransactionDetail>(`/admin/transactions/${id}`);
};
