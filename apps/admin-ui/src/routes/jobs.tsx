import { createRoute } from "@tanstack/react-router";
import JobsDisplay from "@/components/JobsDisplay";
import { rootRoute } from "./root";

function JobsPage() {
  return <JobsDisplay />;
}

export const jobsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/jobs",
  component: JobsPage,
});
