"use client";

import { useState, useMemo } from "react";
import { Button } from "@thalamiq/ui/components/button";
import { Check, ChevronsUpDown } from "lucide-react";
import { cn } from "@thalamiq/ui/utils";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@thalamiq/ui/components/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@thalamiq/ui/components/popover";

export type ComboboxOption = {
    value: string;
    label: string;
};

type ComboboxProps = {
    options: ComboboxOption[];
    value?: string;
    onValueChange: (value: string | undefined) => void;
    placeholder?: string;
    className?: string;
    width?: string;
    allOptionLabel?: string;
    allOptionValue?: string;
};

export const Combobox = ({
    options,
    value,
    onValueChange,
    placeholder = "Select...",
    className,
    width = "200px",
    allOptionLabel = "All",
    allOptionValue = "__all__",
}: ComboboxProps) => {
    const [open, setOpen] = useState(false);

    const selectedOption = useMemo(() => {
        if (!value || value === allOptionValue) {
            return null;
        }
        return options.find((opt) => opt.value === value);
    }, [value, options, allOptionValue]);

    const handleSelect = (selectedValue: string) => {
        const next = selectedValue === allOptionValue ? undefined : selectedValue;
        onValueChange(next);
        setOpen(false);
    };

    const displayValue = selectedOption?.label || (value === allOptionValue ? allOptionLabel : "");

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <Button
                    type="button"
                    variant="outline"
                    role="combobox"
                    aria-expanded={open}
                    className={cn("h-9 justify-between font-normal", !displayValue && "text-muted-foreground", className)}
                    style={{ width }}
                >
                    <span className="text-left whitespace-nowrap overflow-hidden text-ellipsis">
                        {displayValue || placeholder}
                    </span>
                    <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                </Button>
            </PopoverTrigger>
            <PopoverContent className="p-0" style={{ width }}>
                <Command>
                    <CommandInput placeholder="Search..." className="h-9" />
                    <CommandList>
                        <CommandEmpty>No options found.</CommandEmpty>
                        <CommandGroup>
                            <CommandItem
                                value={allOptionValue}
                                onSelect={() => handleSelect(allOptionValue)}
                            >
                                {allOptionLabel}
                                <Check
                                    className={cn(
                                        "ml-auto h-4 w-4",
                                        (!value || value === allOptionValue) ? "opacity-100" : "opacity-0"
                                    )}
                                />
                            </CommandItem>
                            {options.map((option) => (
                                <CommandItem
                                    key={option.value}
                                    value={option.value}
                                    onSelect={() => handleSelect(option.value)}
                                >
                                    {option.label}
                                    <Check
                                        className={cn(
                                            "ml-auto h-4 w-4",
                                            value === option.value ? "opacity-100" : "opacity-0"
                                        )}
                                    />
                                </CommandItem>
                            ))}
                        </CommandGroup>
                    </CommandList>
                </Command>
            </PopoverContent>
        </Popover>
    );
};
