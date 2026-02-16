import { HeaderContext, flexRender } from "@tanstack/react-table";
import { ArrowUpDown, ArrowUp, ArrowDown } from "lucide-react";
import { Button } from "@thalamiq/ui/components/button";
import { cn } from "@thalamiq/ui/utils";

interface ColumnHeaderProps<TData, TValue> {
  context: HeaderContext<TData, TValue>;
  enableSorting?: boolean;
  className?: string;
  children?: React.ReactNode;
}

export function ColumnHeader<TData, TValue>({
  context,
  enableSorting = false,
  className,
  children,
}: ColumnHeaderProps<TData, TValue>) {
  const { column, header } = context;
  const canSort = enableSorting && column.getCanSort();
  const sortDirection = column.getIsSorted();

  // Get header content - prefer children prop, otherwise render from columnDef
  const headerContent =
    children !== undefined
      ? children
      : header.isPlaceholder
        ? null
        : typeof header.column.columnDef.header === "string"
          ? header.column.columnDef.header
          : flexRender(header.column.columnDef.header, context);

  if (!canSort) {
    return (
      <div className={cn("text-xs font-medium", className)}>
        {headerContent}
      </div>
    );
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      className={cn(
        "h-auto p-0 font-medium hover:bg-transparent",
        "-ml-3 h-8 px-3",
        className
      )}
      onClick={() => column.toggleSorting(undefined)}
    >
      <span className="text-xs">{headerContent}</span>
      {sortDirection === "asc" ? (
        <ArrowUp className="ml-2 h-3 w-3" />
      ) : sortDirection === "desc" ? (
        <ArrowDown className="ml-2 h-3 w-3" />
      ) : (
        <ArrowUpDown className="ml-2 h-3 w-3 opacity-50" />
      )}
    </Button>
  );
}
