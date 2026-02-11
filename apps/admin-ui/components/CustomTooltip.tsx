import { TooltipTrigger } from "@thalamiq/ui/components/tooltip";
import { Tooltip } from "@thalamiq/ui/components/tooltip";
import { TooltipContent } from "@thalamiq/ui/components/tooltip";
import React from "react";

interface TooltipProps {
  children: React.ReactNode;
  content: string;
}

const CustomTooltip = ({ children, content }: TooltipProps) => {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent className="max-w-xs text-xs">{content}</TooltipContent>
    </Tooltip>
  );
};

export default CustomTooltip;
