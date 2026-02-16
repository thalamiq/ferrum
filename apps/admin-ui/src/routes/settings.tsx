import { createRoute } from "@tanstack/react-router";
import SettingsDisplay from "@/components/Settings/SettingsDisplay";
import { rootRoute } from "./root";

function SettingsPage() {
  return <SettingsDisplay />;
}

export const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});
