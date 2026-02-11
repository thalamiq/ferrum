import { AlertCircle } from "lucide-react";

const ErrorComp = ({ error }: { error: Error | null | undefined }) => {
  return (
    <div className="flex items-center justify-center h-full w-full">
      <AlertCircle className="w-4 h-4" />
      <p className="text-sm text-destructive">An unexpected error occurred.</p>
    </div>
  );
};

export default ErrorComp;
