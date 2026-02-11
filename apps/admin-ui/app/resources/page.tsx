"use client";

import { useQuery } from "@tanstack/react-query";
import { fetchResources } from "@/lib/api/resources";
import { queryKeys } from "@/lib/api/query-keys";
import { ErrorArea } from "@/components/Error";
import { LoadingArea } from "@/components/Loading";
import { ResourcesDisplay } from "@/components/ResourcesDisplay";

const ResourcesPage = () => {
  const resourcesQuery = useQuery({
    queryKey: queryKeys.resources,
    queryFn: fetchResources,
  });

  if (resourcesQuery.isPending) {
    return <LoadingArea />;
  }

  if (resourcesQuery.isError) {
    return <ErrorArea error={resourcesQuery.error} />;
  }

  if (!resourcesQuery.data) {
    return null;
  }

  return <ResourcesDisplay report={resourcesQuery.data} />;
};

export default ResourcesPage;
