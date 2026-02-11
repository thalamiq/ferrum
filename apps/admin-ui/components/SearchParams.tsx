"use client";

import { useState } from "react";
import { Button } from "@thalamiq/ui/components/button";
import { Filter, ChevronDown } from "lucide-react";
import CustomTooltip from "@/components/CustomTooltip";
import { truncateString } from "@/lib/utils";

type SearchParamInfo = {
  name: string;
  documentation?: string | null;
};

type GroupedParams = {
  resourceSpecific: SearchParamInfo[];
  common: SearchParamInfo[];
  control: SearchParamInfo[];
};

interface SearchParamsProps {
  groupedParams: GroupedParams;
  onParamClick: (paramName: string) => void;
  actionButtons?: React.ReactNode;
}

export default function SearchParams({
  groupedParams,
  onParamClick,
  actionButtons,
}: SearchParamsProps) {
  const [open, setOpen] = useState(false);

  const totalParams =
    groupedParams.resourceSpecific.length +
    groupedParams.common.length +
    groupedParams.control.length;

  if (totalParams === 0 && !actionButtons) return null;

  const renderParamGroup = (
    params: SearchParamInfo[],
    title: string,
    description?: string
  ) => {
    if (params.length === 0) return null;

    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <h4 className="text-sm font-semibold text-foreground">{title}</h4>
          {description && (
            <span className="text-xs text-muted-foreground">{description}</span>
          )}
          <span className="ml-auto text-xs text-muted-foreground">
            {params.length}
          </span>
        </div>
        <div className="flex flex-wrap gap-2">
          {params.map(({ name, documentation }) => (
            <CustomTooltip
              key={name}
              content={truncateString(documentation || "No documentation", 100)}
            >
              <Button
                variant="outline"
                size="sm"
                onClick={() => onParamClick(name)}
                className="h-7 text-xs hover:bg-primary/10"
              >
                {name}
              </Button>
            </CustomTooltip>
          ))}
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        {totalParams > 0 && (
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="flex flex-1 items-center justify-between rounded-md border px-3 py-2 text-left text-sm hover:bg-muted/60 transition-colors bg-card"
          >
            <span className="flex items-center gap-2 text-muted-foreground">
              <Filter className="h-4 w-4" />
              Search parameters
              <span className="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs text-foreground">
                {totalParams}
              </span>
            </span>
            <ChevronDown
              className={`h-4 w-4 transition-transform ${
                open ? "rotate-180" : "rotate-0"
              }`}
            />
          </button>
        )}
        {actionButtons && (
          <div className="flex gap-2 shrink-0">
            {actionButtons}
          </div>
        )}
      </div>
      {open && (
        <div className="space-y-4 p-3 border rounded-md bg-card">
          {renderParamGroup(
            groupedParams.resourceSpecific,
            "Resource-Specific Parameters",
            "Parameters defined for this resource type"
          )}
          {renderParamGroup(
            groupedParams.common,
            "Common Parameters",
            "Parameters defined for all resources"
          )}
          {renderParamGroup(
            groupedParams.control,
            "Search Control Parameters",
            "Parameters that control search behavior and result format"
          )}
        </div>
      )}
    </div>
  );
}
