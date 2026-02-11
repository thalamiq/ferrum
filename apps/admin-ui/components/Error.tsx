import { AlertCircle } from "lucide-react";

export const ErrorArea = ({ error }: { error: Error }) => {
  return (
    <div className="flex items-center justify-center h-full w-full">
      <AlertCircle className="w-4 h-4" />
      <p className="text-sm text-destructive">An unexpected error occurred.</p>
    </div>
  );
};
