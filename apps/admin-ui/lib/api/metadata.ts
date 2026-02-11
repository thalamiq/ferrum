import { CapabilityStatement } from "fhir/r4";
import { getFetcher } from "./fetcher";

export const fetchMetadata = async ({
  mode = "full",
}: {
  mode: "full" | "normative" | "terminology";
}): Promise<CapabilityStatement> => {
  return getFetcher<CapabilityStatement>(`/api/fhir/metadata?mode=${mode}`);
};
