import { getFetcher } from "./fetcher";

export interface ResourceTypeStats {
  resourceType: string;
  totalVersions: number;
  currentTotal: number;
  currentActive: number;
  currentDeleted: number;
  lastUpdated: string;
}

export interface ResourceTypeStatsTotals {
  resourceTypeCount: number;
  totalVersions: number;
  currentTotal: number;
  currentActive: number;
  currentDeleted: number;
  lastUpdated: string;
}

export interface ResourceTypeStatsReport {
  resourceTypes: ResourceTypeStats[];
  totals: ResourceTypeStatsTotals;
}

export const fetchResources = async () => {
  return getFetcher<ResourceTypeStatsReport>("/api/admin/resources/stats");
};
