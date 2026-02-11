import { getFetcher } from "./fetcher";

export interface HealthResponse {
  status: string;
  service: string;
}

export const fetchHealth = async (): Promise<HealthResponse> => {
  return getFetcher<HealthResponse>("/api/health");
};
