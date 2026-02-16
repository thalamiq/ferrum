import { createRoute } from "@tanstack/react-router";
import DashboardDisplay from "@/components/DashboardDisplay";
import { rootRoute } from "./root";

function DashboardPage() {
  return <DashboardDisplay />;
}

export const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dashboard",
  component: DashboardPage,
});
