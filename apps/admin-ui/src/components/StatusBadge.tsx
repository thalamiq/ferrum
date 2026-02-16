import { Badge } from "@thalamiq/ui/components/badge";
import { CheckCircle, XCircle, AlertCircle } from "lucide-react";

interface StatusBadgeProps {
  status: number;
}

export default function StatusBadge({ status }: StatusBadgeProps) {
  if (status >= 200 && status < 300) {
    return (
      <Badge className="bg-green-500 hover:bg-green-600">
        <CheckCircle className="h-3 w-3 mr-1" />
        {status}
      </Badge>
    );
  }

  if (status >= 400 && status < 500) {
    return (
      <Badge variant="destructive">
        <XCircle className="h-3 w-3 mr-1" />
        {status}
      </Badge>
    );
  }

  if (status >= 500) {
    return (
      <Badge variant="destructive">
        <AlertCircle className="h-3 w-3 mr-1" />
        {status}
      </Badge>
    );
  }

  return <Badge variant="outline">{status}</Badge>;
}
