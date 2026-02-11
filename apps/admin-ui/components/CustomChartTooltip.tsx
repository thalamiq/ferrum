import { formatNumber } from "@/lib/utils";

interface CustomChartTooltipProps {
  payload?: any;
  label?: string | number;
  chartConfig: any;
}

const CustomChartTooltip = ({
  payload,
  label,
  chartConfig,
}: CustomChartTooltipProps) => {
  return (
    <div className="rounded-lg border bg-background p-2 shadow-sm">
      <div className="grid gap-2">
        <div className="font-medium text-sm">{label || "Resource"}</div>
        {payload?.map((entry: any, index: number) => (
          <div
            key={index}
            className="flex items-center justify-between gap-4 text-sm"
          >
            <div className="flex items-center gap-2">
              <div
                className="h-2.5 w-2.5 rounded-full"
                style={{ backgroundColor: entry.color }}
              />
              <span className="text-muted-foreground">
                {chartConfig[entry.dataKey as keyof typeof chartConfig]
                  ?.label || entry.dataKey}
              </span>
            </div>
            <span className="font-medium tabular-nums">
              {formatNumber(Number(entry.value))}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
};

export default CustomChartTooltip;
