import { createRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getPackage, getPackageResources } from "@/api/packages";
import { queryKeys } from "@/api/query-keys";
import { ErrorArea } from "@/components/Error";
import { LoadingArea } from "@/components/Loading";
import { PackageDetailDisplay } from "@/components/PackageDetailDisplay";
import { useState } from "react";
import { rootRoute } from "./root";

const PACKAGE_DETAIL_PAGE_SIZE = 50;

function PackageDetailPage() {
  const { packageId } = packageDetailRoute.useParams();
  const [offset, setOffset] = useState(0);

  const packageQuery = useQuery({
    queryKey: queryKeys.package(packageId),
    queryFn: () => getPackage(packageId),
    enabled: !!packageId,
  });

  const resourcesQuery = useQuery({
    queryKey: queryKeys.packageResources(
      parseInt(packageId, 10),
      false,
      undefined,
      PACKAGE_DETAIL_PAGE_SIZE,
      offset,
    ),
    queryFn: () =>
      getPackageResources({
        packageId: parseInt(packageId, 10),
        limit: PACKAGE_DETAIL_PAGE_SIZE,
        offset,
      }),
    enabled: !!packageId && packageQuery.isSuccess,
  });

  if (packageQuery.isPending) {
    return <LoadingArea />;
  }

  if (packageQuery.isError) {
    return <ErrorArea error={packageQuery.error} />;
  }

  if (!packageQuery.data) {
    return null;
  }

  if (resourcesQuery.isPending) {
    return <LoadingArea />;
  }

  if (resourcesQuery.isError) {
    return <ErrorArea error={resourcesQuery.error} />;
  }

  if (!resourcesQuery.data) {
    return null;
  }

  return (
    <PackageDetailDisplay
      packageData={packageQuery.data}
      resourcesData={resourcesQuery.data}
      onPageChange={setOffset}
      pageSize={PACKAGE_DETAIL_PAGE_SIZE}
    />
  );
}

export const packageDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/packages/$packageId",
  component: PackageDetailPage,
});
