import { createRoute } from "@tanstack/react-router";
import SearchCoverageDisplay from "@/components/SearchParameters/SearchCoverageDisplay";
import { rootRoute } from "../root";

function SearchCoveragePage() {
  return <SearchCoverageDisplay />;
}

export const searchCoverageRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search/search-coverage",
  component: SearchCoveragePage,
});
