import { redirect } from "next/navigation";
import { config } from "@/lib/config";

export default function HomePage() {
  redirect(config.nav.dashboard.path);
  return null;
}
