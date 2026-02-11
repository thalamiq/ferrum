"use client";

import { useEffect, useState } from "react";
import { useRouter, usePathname } from "next/navigation";
import { hasSession } from "@/lib/auth";
import { fetchUiConfig } from "@/lib/config";
import { LoadingArea } from "./Loading";

export function AuthGuard({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const [isChecking, setIsChecking] = useState(true);
  const [requiresAuth, setRequiresAuth] = useState(false);
  const [isAuthed, setIsAuthed] = useState(false);

  useEffect(() => {
    async function checkAuth() {
      try {
        // Fetch UI config to see if auth is required
        const config = await fetchUiConfig();
        setRequiresAuth(config.requires_auth);

        const sessionOk = config.requires_auth ? await hasSession() : true;
        setIsAuthed(sessionOk);

        // If auth is required and user has no session, redirect to login
        if (config.requires_auth && !sessionOk && pathname !== "/login") {
          router.push("/login");
          return;
        }

        // If user is authenticated and trying to access login, redirect to dashboard
        if (sessionOk && pathname === "/login") {
          router.push("/dashboard");
          return;
        }
      } catch (error) {
        console.error("Auth check failed:", error);
      } finally {
        setIsChecking(false);
      }
    }

    checkAuth();
  }, [router, pathname]);

  // Show loading state while checking
  if (isChecking) {
    return <LoadingArea />;
  }

  // If on login page or auth not required, show content
  if (pathname === "/login" || !requiresAuth || isAuthed) {
    return <>{children}</>;
  }

  // This shouldn't be reached due to redirect, but just in case
  return null;
}
