import {
  EmptyContent,
  Empty,
  EmptyDescription,
} from "@thalamiq/ui/components/empty";
import { AlertCircle } from "lucide-react";

const EmptyDisplay = () => {
  return (
    <Empty>
      <EmptyContent>
        <AlertCircle className="w-4 h-4" />
        <EmptyDescription>No data found</EmptyDescription>
      </EmptyContent>
    </Empty>
  );
};

export default EmptyDisplay;
