import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { Providers } from "@/components/Providers";
import { cookies } from "next/headers";
import { ConditionalLayout } from "@/components/ConditionalLayout";
import "./globals.css";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-inter",
});

export const metadata: Metadata = {
  title: {
    default: "Zunder",
    template: "%s · Zunder",
  },
  applicationName: "Zunder",
  description: "Admin UI for the Zunder FHIR Server",
  icons: {
    icon: "/icon.svg",
  },
};

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const cookieStore = await cookies();
  const defaultOpen = cookieStore.get("sidebar_state")?.value === "true";

  return (
    <html lang="en" suppressHydrationWarning>
      <body className={`${inter.variable} antialiased h-screen bg-sidebar`}>
        <Providers>
          <ConditionalLayout defaultOpen={defaultOpen}>
            {children}
          </ConditionalLayout>
        </Providers>
      </body>
    </html>
  );
}
