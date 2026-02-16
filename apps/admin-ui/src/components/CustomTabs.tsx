import React from "react";
import { cn } from "@thalamiq/ui/utils";

interface CustomTabsListProps {
  children: React.ReactNode;
  className?: string;
}

export const CustomTabsList = ({
  children,
  className,
}: CustomTabsListProps) => {
  return (
    <div
      className={cn(
        "inline-flex h-7 items-center justify-center rounded-lg bg-muted p-1 text-muted-foreground",
        className
      )}
    >
      {children}
    </div>
  );
};

interface CustomTabsTriggerProps {
  children: React.ReactNode;
  value?: string;
  active: boolean;
  onClick: () => void;
  className?: string;
}

export const CustomTabsTrigger = ({
  children,
  active,
  onClick,
  className,
}: CustomTabsTriggerProps) => {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex items-center justify-center whitespace-nowrap rounded-md px-2 py-1 text-xs font-medium ring-offset-background transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50",
        active
          ? "bg-background text-foreground shadow"
          : "text-muted-foreground hover:text-foreground",
        className
      )}
      data-state={active ? "active" : "inactive"}
    >
      {children}
    </button>
  );
};
