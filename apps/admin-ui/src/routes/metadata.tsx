import { createRoute } from "@tanstack/react-router";
import { fetchMetadata } from "@/api/metadata";
import { queryKeys } from "@/api/query-keys";
import { ErrorArea } from "@/components/Error";
import { LoadingArea } from "@/components/Loading";
import { CapabilityStatementDisplay } from "@/components/CapabilityStatementDisplay";
import { useQuery } from "@tanstack/react-query";
import { rootRoute } from "./root";

function MetadataPage() {
  const metadataQuery = useQuery({
    queryKey: queryKeys.metadata("full"),
    queryFn: () => fetchMetadata({ mode: "full" }),
  });

  if (metadataQuery.isPending) {
    return <LoadingArea />;
  }

  if (metadataQuery.isError) {
    return <ErrorArea error={metadataQuery.error} />;
  }

  if (!metadataQuery.data) {
    return null;
  }

  return (
    <CapabilityStatementDisplay capabilityStatement={metadataQuery.data} />
  );
}

export const metadataRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/metadata",
  component: MetadataPage,
});
