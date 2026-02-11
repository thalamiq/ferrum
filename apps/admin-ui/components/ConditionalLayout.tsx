"use client";

import { usePathname } from "next/navigation";
import { SidebarInset, SidebarProvider } from "@thalamiq/ui/components/sidebar";
import AppSidebar from "@/components/AppSidebar";
import { ConnectionGuard } from "@/components/ConnectionGuard";
import { AuthGuard } from "@/components/AuthGuard";
import { Toaster } from "sonner";

interface ConditionalLayoutProps {
  children: React.ReactNode;
  defaultOpen: boolean;
}

export function ConditionalLayout({ children, defaultOpen }: ConditionalLayoutProps) {
  const pathname = usePathname();
  const isLoginPage = pathname === "/login";

  if (isLoginPage) {
    // Login page without sidebar
    return (
      <AuthGuard>
        {children}
        <Toaster />
      </AuthGuard>
    );
  }

  // Regular pages with sidebar
  return (
    <AuthGuard>
      <SidebarProvider defaultOpen={defaultOpen}>
        <AppSidebar />
        <SidebarInset className="flex flex-col overflow-hidden">
          <main className="flex-1 min-h-0 overflow-y-auto">
            <ConnectionGuard>{children}</ConnectionGuard>
          </main>
          <Toaster />
        </SidebarInset>
      </SidebarProvider>
    </AuthGuard>
  );
}
