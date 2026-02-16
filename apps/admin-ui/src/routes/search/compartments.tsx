import { createRoute } from "@tanstack/react-router";
import SearchCompartmentsDisplay from "@/components/SearchParameters/SearchCompartmentsDisplay";
import { rootRoute } from "../root";

function SearchCompartmentsPage() {
  return <SearchCompartmentsDisplay />;
}

export const compartmentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search/compartments",
  component: SearchCompartmentsPage,
});
