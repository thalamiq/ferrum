import { createRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { fetchResources } from "@/api/resources";
import { queryKeys } from "@/api/query-keys";
import { ErrorArea } from "@/components/Error";
import { LoadingArea } from "@/components/Loading";
import { ResourcesDisplay } from "@/components/ResourcesDisplay";
import { rootRoute } from "./root";

function ResourcesPage() {
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
}

export const resourcesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/resources",
  component: ResourcesPage,
});
