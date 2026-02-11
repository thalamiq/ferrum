import { formatFileSize } from "@/lib/json";
import { Button } from "@thalamiq/ui/components/button";
import { cn } from "@thalamiq/ui/utils";
import {
  Check,
  Copy,
  Download,
  Minimize2,
  Maximize2,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from "lucide-react";
import React, { useCallback, useMemo, useState } from "react";
import CustomTooltip from "@/components/CustomTooltip";
import { Badge } from "@thalamiq/ui/components/badge";
import { toast } from "sonner";

interface BundleLink {
  relation: string;
  url: string;
}

interface JsonToolbarProps {
  isFullscreen?: boolean;
  toggleFullscreen?: () => void;
  data: unknown;
  tabsSlot?: React.ReactNode;
  bundleLinks?: BundleLink[];
  onNavigate?: (url: string) => void;
}

const JsonToolbar = ({
  isFullscreen = false,
  toggleFullscreen,
  data,
  tabsSlot,
  bundleLinks = [],
  onNavigate,
}: JsonToolbarProps) => {
  const jsonString = useMemo(() => JSON.stringify(data, null, 2), [data]);
  const [copied, setCopied] = useState(false);
  const dataSize = useMemo(() => new Blob([jsonString]).size, [jsonString]);

  // Extract pagination links
  const paginationLinks = useMemo(() => {
    const links: Record<string, string> = {};
    bundleLinks.forEach((link) => {
      if (["first", "prev", "next", "last"].includes(link.relation)) {
        links[link.relation] = link.url;
      }
    });
    return links;
  }, [bundleLinks]);

  const hasPagination = Object.keys(paginationLinks).length > 0;

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(jsonString);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Failed to copy JSON");
    }
  }, [jsonString]);

  const handleDownload = useCallback(() => {
    const blob = new Blob([jsonString], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "data.json";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    toast.success("JSON downloaded");
  }, [jsonString]);

  return (
    <div
      className={cn(
        "flex items-center justify-between shrink-0 py-2",
        isFullscreen && "bg-muted/30"
      )}
    >
      <div className="flex items-center gap-3">
        {tabsSlot}
        {isFullscreen && (
          <Badge variant="secondary" className="text-xs ml-2">
            FULLSCREEN
          </Badge>
        )}
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span>{formatFileSize(dataSize)}</span>
        </div>
      </div>

      <div className="flex items-center gap-1">
        {hasPagination && onNavigate && (
          <>
            <CustomTooltip content="First">
              <Button
                variant="ghost"
                size="icon"
                onClick={() =>
                  paginationLinks.first && onNavigate(paginationLinks.first)
                }
                disabled={!paginationLinks.first}
                className="h-8 w-8"
              >
                <ChevronsLeft className="w-3 h-3" />
              </Button>
            </CustomTooltip>
            <CustomTooltip content="Previous">
              <Button
                variant="ghost"
                size="icon"
                onClick={() =>
                  paginationLinks.prev && onNavigate(paginationLinks.prev)
                }
                disabled={!paginationLinks.prev}
                className="h-8 w-8"
              >
                <ChevronLeft className="w-3 h-3" />
              </Button>
            </CustomTooltip>
            <CustomTooltip content="Next">
              <Button
                variant="ghost"
                size="icon"
                onClick={() =>
                  paginationLinks.next && onNavigate(paginationLinks.next)
                }
                disabled={!paginationLinks.next}
                className="h-8 w-8"
              >
                <ChevronRight className="w-3 h-3" />
              </Button>
            </CustomTooltip>
            <CustomTooltip content="Last">
              <Button
                variant="ghost"
                size="icon"
                onClick={() =>
                  paginationLinks.last && onNavigate(paginationLinks.last)
                }
                disabled={!paginationLinks.last}
                className="h-8 w-8"
              >
                <ChevronsRight className="w-3 h-3" />
              </Button>
            </CustomTooltip>
            <div className="w-px h-6 bg-border mx-1" />
          </>
        )}
        <CustomTooltip content="Copy">
          <Button
            variant="ghost"
            size="icon"
            onClick={handleCopy}
            className="h-8 w-8"
          >
            {copied ? (
              <Check className="w-3 h-3" />
            ) : (
              <Copy className="w-3 h-3" />
            )}
          </Button>
        </CustomTooltip>
        <CustomTooltip content="Download">
          <Button
            variant="ghost"
            size="icon"
            onClick={handleDownload}
            className="h-8 w-8"
          >
            <Download className="w-3 h-3" />
          </Button>
        </CustomTooltip>
        {toggleFullscreen && (
          <CustomTooltip
            content={isFullscreen ? "Exit Fullscreen" : "Enter Fullscreen"}
          >
            <Button
              variant="ghost"
              size="icon"
              onClick={toggleFullscreen}
              className="h-8 w-8"
            >
              {isFullscreen ? (
                <Minimize2 className="w-3 h-3" />
              ) : (
                <Maximize2 className="w-3 h-3" />
              )}
            </Button>
          </CustomTooltip>
        )}
      </div>
    </div>
  );
};

export default JsonToolbar;
