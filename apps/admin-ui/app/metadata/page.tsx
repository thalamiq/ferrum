"use client";

import { fetchMetadata } from "@/lib/api/metadata";
import { queryKeys } from "@/lib/api/query-keys";
import { ErrorArea } from "@/components/Error";
import { LoadingArea } from "@/components/Loading";
import { CapabilityStatementDisplay } from "@/components/CapabilityStatementDisplay";
import { useQuery } from "@tanstack/react-query";

export default function Home() {
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
