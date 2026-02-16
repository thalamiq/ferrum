import { useState, useMemo } from "react";
import { useNavigate } from "@tanstack/react-router";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@thalamiq/ui/components/card";
import { Badge } from "@thalamiq/ui/components/badge";
import SearchInput from "./SearchInput";
import { PackageListResponse } from "@/api/packages";
import { CheckCircle, XCircle, Clock, Package } from "lucide-react";
import { config } from "@/lib/config";

interface PackagesDisplayProps {
  data: PackageListResponse;
}

const formatDate = (dateString: string | null | undefined): string => {
  if (!dateString) return "Never";
  try {
    const date = new Date(dateString);
    return date.toLocaleString();
  } catch {
    return dateString;
  }
};

const formatNumber = (num: number): string => {
  return new Intl.NumberFormat().format(num);
};

const getStatusBadge = (status: string, errorMessage?: string) => {
  const statusLower = status.toLowerCase();

  if (statusLower === "completed" || statusLower === "active") {
    return (
      <Badge className="bg-success">
        <CheckCircle className="h-3 w-3 mr-1" />
        {status}
      </Badge>
    );
  }

  if (statusLower === "failed" || statusLower === "error") {
    return (
      <Badge variant="destructive">
        <XCircle className="h-3 w-3 mr-1" />
        {status}
      </Badge>
    );
  }

  if (
    statusLower === "pending" ||
    statusLower === "processing" ||
    statusLower === "installing"
  ) {
    return (
      <Badge variant="secondary">
        <Clock className="h-3 w-3 mr-1" />
        {status}
      </Badge>
    );
  }

  return <Badge variant="outline">{status}</Badge>;
};

export const PackagesDisplay = ({ data }: PackagesDisplayProps) => {
  const navigate = useNavigate();
  const { packages, total, limit, offset } = data;
  const [packageFilter, setPackageFilter] = useState("");

  // Get unique statuses for filtering
  const uniqueStatuses = useMemo(() => {
    const statuses = new Set(packages.map((pkg) => pkg.status));
    return Array.from(statuses).sort();
  }, [packages]);

  // Filter function for packages
  const filteredPackages = useMemo(() => {
    let filtered = packages;

    if (packageFilter) {
      const filter = packageFilter.toLowerCase();
      filtered = filtered.filter(
        (pkg) =>
          pkg.name.toLowerCase().includes(filter) ||
          pkg.version.toLowerCase().includes(filter) ||
          pkg.status.toLowerCase().includes(filter)
      );
    }

    return filtered;
  }, [packages, packageFilter]);

  // Calculate statistics
  const stats = useMemo(() => {
    const byStatus = packages.reduce((acc, pkg) => {
      const status = pkg.status.toLowerCase();
      acc[status] = (acc[status] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);

    const totalResources = packages.reduce(
      (sum, pkg) => sum + pkg.resourceCount,
      0
    );

    return {
      byStatus,
      totalResources,
      completed: byStatus["completed"] || 0,
      failed: byStatus["failed"] || byStatus["error"] || 0,
      pending:
        byStatus["pending"] ||
        byStatus["processing"] ||
        byStatus["installing"] ||
        0,
    };
  }, [packages]);

  return (
    <div className="space-y-4">
      {/* Packages Cards */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Packages</CardTitle>
              <CardDescription className="mt-1">
                {filteredPackages.length} package
                {filteredPackages.length !== 1 ? "s" : ""}
                {filteredPackages.length !== packages.length &&
                  ` of ${packages.length}`}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Filters */}
          <div className="flex flex-col sm:flex-row gap-4">
            <SearchInput
              searchQuery={packageFilter}
              setSearchQuery={setPackageFilter}
              placeholder="Filter packages by name, version, or status..."
            />
          </div>

          {filteredPackages.length === 0 ? (
            <div className="text-sm text-muted-foreground text-center py-8">
              No packages match your filter.
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-4 gap-4">
              {filteredPackages.map((pkg) => (
                <Card
                  key={pkg.id}
                  className="cursor-pointer hover:bg-accent/50 transition-colors min-w-0"
                  onClick={() =>
                    navigate({ to: `${config.nav.packages.path}/${pkg.id}` })
                  }
                >
                  <CardHeader className="pb-3 overflow-hidden min-w-0">
                    <div className="flex items-start justify-between gap-2 w-full min-w-0">
                      <div className="flex-1 min-w-0 overflow-hidden">
                        <CardTitle className="text-base truncate min-w-0 block">
                          {pkg.name}
                        </CardTitle>
                        <CardDescription className="mt-1 truncate min-w-0 overflow-hidden block whitespace-nowrap">
                          Version: {pkg.version}
                        </CardDescription>
                      </div>
                      <Package className="h-5 w-5 text-muted-foreground shrink-0" />
                    </div>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <div className="space-y-2 text-sm">
                      <div className="flex justify-between items-center">
                        <span className="text-muted-foreground">Status:</span>
                        {getStatusBadge(pkg.status, pkg.errorMessage)}
                      </div>
                      <div className="flex justify-between items-center">
                        <span className="text-muted-foreground">
                          Resources:
                        </span>
                        <span className="font-medium">
                          {formatNumber(pkg.resourceCount)}
                        </span>
                      </div>
                      <div className="flex justify-between items-center">
                        <span className="text-muted-foreground">Created:</span>
                        <span className="text-xs text-muted-foreground">
                          {formatDate(pkg.createdAt)}
                        </span>
                      </div>
                    </div>
                    {pkg.errorMessage && (
                      <div className="pt-2 border-t">
                        <div className="text-xs text-destructive">
                          <div className="font-medium mb-1">Error:</div>
                          <div className="wrap-break-word">
                            {pkg.errorMessage}
                          </div>
                        </div>
                      </div>
                    )}
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
};
