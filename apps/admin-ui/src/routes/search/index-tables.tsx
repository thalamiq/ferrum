import { createRoute } from "@tanstack/react-router";
import SearchIndexTable from "@/components/SearchParameters/SearchIndexTablesDisplay";
import { rootRoute } from "../root";

function IndexTablesPage() {
  return <SearchIndexTable />;
}

export const indexTablesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search/index-tables",
  component: IndexTablesPage,
});
