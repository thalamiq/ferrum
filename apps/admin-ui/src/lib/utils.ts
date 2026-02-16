import { format, formatDistanceToNow, differenceInHours } from "date-fns";

export const formatDate = (iso: string | null | undefined): string => {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return format(date, "yyyy-MM-dd");
};

export const formatDateTime = (
  dateString: string | null | undefined
): string => {
  if (!dateString) return "—";
  const date = new Date(dateString);
  if (Number.isNaN(date.getTime())) return dateString;
  
  // For dates within the last 24 hours, show relative time
  const hoursDiff = Math.abs(differenceInHours(date, new Date()));
  if (hoursDiff < 24) {
    return formatDistanceToNow(date, { addSuffix: true });
  }
  
  // For older dates, show formatted date/time
  return format(date, "MMM d, yyyy 'at' h:mm:ss a");
};

export const formatDateTimeFull = (
  dateString: string | null | undefined
): string => {
  if (!dateString) return "—";
  const date = new Date(dateString);
  if (Number.isNaN(date.getTime())) return dateString;
  return format(date, "MMM d, yyyy 'at' h:mm:ss a");
};

export const truncateString = (str: string, length: number) => {
  return str.length > length ? str.slice(0, length) + "..." : str;
};

export const formatNumber = (num: number): string => {
  return new Intl.NumberFormat().format(num);
};
