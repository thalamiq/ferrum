import { createRoute } from "@tanstack/react-router";
import APIDisplay from "@/components/ApiDisplay";
import { rootRoute } from "./root";

function RequestsPage() {
  return (
    <div className="h-full">
      <APIDisplay />
    </div>
  );
}

export const requestsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/requests",
  component: RequestsPage,
});
