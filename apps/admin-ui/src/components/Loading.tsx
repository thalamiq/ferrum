import { Loader2 } from "lucide-react";

export const LoadingSpinner = () => {
  return <Loader2 className="w-10 h-10 animate-spin text-primary" />;
};

export const LoadingArea = () => {
  return (
    <div className="flex items-center justify-center h-full w-full">
      <LoadingSpinner />
    </div>
  );
};
