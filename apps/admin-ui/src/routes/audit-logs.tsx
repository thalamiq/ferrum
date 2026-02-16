import { createRoute } from "@tanstack/react-router";
import AuditEventDisplay from "@/components/AuditEventDisplay";
import { rootRoute } from "./root";

function AuditLogsPage() {
  return <AuditEventDisplay />;
}

export const auditLogsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/audit-logs",
  component: AuditLogsPage,
});
