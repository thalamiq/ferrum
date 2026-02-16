import { createRoute } from "@tanstack/react-router";
import SearchParameterDisplay from "@/components/SearchParameters/SearchParameterDisplay";
import { rootRoute } from "../root";

function SearchParametersPage() {
  return <SearchParameterDisplay />;
}

export const searchParametersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search/search-parameters",
  component: SearchParametersPage,
});
