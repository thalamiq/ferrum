import { CapabilityStatement } from "fhir/r4";
import { getFetcher } from "./client";

export const fetchMetadata = async ({
  mode = "full",
}: {
  mode: "full" | "normative" | "terminology";
}): Promise<CapabilityStatement> => {
  return getFetcher<CapabilityStatement>(`/fhir/metadata?mode=${mode}`);
};
