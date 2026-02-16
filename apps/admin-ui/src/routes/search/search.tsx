import { createRoute, redirect } from "@tanstack/react-router";
import { rootRoute } from "../root";

export const searchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search",
  beforeLoad: () => {
    throw redirect({ to: "/search/search-parameters" });
  },
});
