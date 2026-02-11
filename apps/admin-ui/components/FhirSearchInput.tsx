"use client";

import { useState, useEffect, useMemo, useRef } from "react";
import { CapabilityStatement } from "fhir/r4";
import { Input } from "@thalamiq/ui/components/input";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@thalamiq/ui/components/command";
import {
  Database,
  Filter,
  Slash,
  HelpCircle,
  Hash,
  Settings,
  X,
} from "lucide-react";
import { cn } from "@thalamiq/ui/utils";

const SEARCH_PREFIXES = [
  "eq",
  "ne",
  "gt",
  "lt",
  "ge",
  "le",
  "sa",
  "eb",
  "ap",
] as const;
const SEARCH_MODIFIERS = [
  ":exact",
  ":contains",
  ":not",
  ":missing",
  ":text",
  ":in",
  ":not-in",
  ":below",
  ":above",
  ":type",
] as const;

// Common Parameters defined for all resources
const COMMON_PARAMS = [
  { name: "_content", type: "string" },
  { name: "_filter", type: "special" },
  { name: "_has", type: "special" },
  { name: "_id", type: "token" },
  { name: "_in", type: "reference" },
  { name: "_language", type: "token" },
  { name: "_lastUpdated", type: "date" },
  { name: "_list", type: "string" },
  { name: "_profile", type: "reference" },
  { name: "_query", type: "string" },
  { name: "_security", type: "token" },
  { name: "_source", type: "uri" },
  { name: "_tag", type: "token" },
  { name: "_text", type: "string" },
];

// Search Control Parameters
const SEARCH_CONTROL_PARAMS = [
  { name: "_contained", type: "string" },
  { name: "_containedType", type: "string" },
  { name: "_count", type: "number" },
  { name: "_elements", type: "token" },
  { name: "_graph", type: "reference" },
  { name: "_include", type: "string" },
  { name: "_maxresults", type: "number" },
  { name: "_revinclude", type: "string" },
  { name: "_score", type: "token" },
  { name: "_sort", type: "string" },
  { name: "_summary", type: "string" },
  { name: "_total", type: "token" },
];

interface FhirSearchInputProps {
  searchQuery: string;
  setSearchQuery: (searchQuery: string) => void;
  capabilityStatement?: CapabilityStatement;
  inputClassName?: string;
}

export default function FhirSearchInput({
  searchQuery,
  setSearchQuery,
  inputClassName,
  capabilityStatement,
}: FhirSearchInputProps) {
  const [open, setOpen] = useState(false);
  const [inputValue, setInputValue] = useState(searchQuery);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [selectedValue, setSelectedValue] = useState<string | null>(null);
  const commandListRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  const availableResources = useMemo(() => {
    return capabilityStatement?.rest?.[0]?.resource || [];
  }, [capabilityStatement]);

  const globalSearchParams = useMemo(() => {
    return capabilityStatement?.rest?.[0]?.searchParam || [];
  }, [capabilityStatement]);

  const getCombinedSearchParams = useMemo(() => {
    return (resourceType: string) => {
      const resource = availableResources.find((r) => r.type === resourceType);
      const resourceParams = resource?.searchParam || [];
      const allParams = [...resourceParams, ...globalSearchParams];
      const paramNames = new Set(allParams.map((p) => p.name));

      // Create sets for quick lookup
      const commonParamNames = new Set(COMMON_PARAMS.map((p) => p.name));
      const controlParamNames = new Set(
        SEARCH_CONTROL_PARAMS.map((p) => p.name)
      );
      const resourceParamNames = new Set(resourceParams.map((p) => p.name));

      // Add common params if not already included
      const commonParamsToAdd = COMMON_PARAMS.filter(
        (p) => !paramNames.has(p.name)
      ).map((p) => ({
        name: p.name,
        type: p.type,
        documentation: undefined,
        category: "common" as const,
      }));

      // Add control params if not already included
      const controlParamsToAdd = SEARCH_CONTROL_PARAMS.filter(
        (p) => !paramNames.has(p.name)
      ).map((p) => ({
        name: p.name,
        type: p.type,
        documentation: undefined,
        category: "result" as const,
      }));

      // Categorize existing params
      const categorizedParams = allParams.map((p) => ({
        ...p,
        category: commonParamNames.has(p.name)
          ? ("common" as const)
          : controlParamNames.has(p.name)
          ? ("result" as const)
          : resourceParamNames.has(p.name)
          ? ("resource" as const)
          : ("resource" as const), // Default to resource for global params
      }));

      return [
        ...categorizedParams,
        ...commonParamsToAdd,
        ...controlParamsToAdd,
      ];
    };
  }, [availableResources, globalSearchParams]);

  // Parse FHIR path into components
  const parseQuery = (query: string) => {
    const trimmed = query.trim();
    if (!trimmed) {
      return {
        path: "",
        resourceType: null,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params: [],
      };
    }

    const [pathPart, queryParams = ""] = trimmed.split("?");
    const params = queryParams.split("&").filter((p) => p.length > 0);
    const pathSegments = pathPart.split("/").filter((s) => s.length > 0);

    // System-level paths
    if (pathPart === "" || pathPart === "/") {
      return {
        path: pathPart,
        resourceType: null,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: true,
        params,
      };
    }

    // Metadata
    if (pathSegments[0] === "metadata") {
      return {
        path: pathPart,
        resourceType: null,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: true,
        isSystem: false,
        params,
      };
    }

    // System-level operations: /$operation
    if (pathSegments[0]?.startsWith("$")) {
      return {
        path: pathPart,
        resourceType: null,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: pathSegments[0].substring(1),
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: true,
        params,
      };
    }

    // System-level search: /_search
    if (pathSegments[0] === "_search") {
      return {
        path: pathPart,
        resourceType: null,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: false,
        isVHistory: false,
        isSearch: true,
        isMetadata: false,
        isSystem: true,
        params,
      };
    }

    // System-level history: /_history
    if (pathSegments[0] === "_history") {
      return {
        path: pathPart,
        resourceType: null,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: true,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: true,
        params,
      };
    }

    // Compartment paths: /[compartment]/[id]/*
    if (pathSegments.length >= 2) {
      const firstSegment = pathSegments[0];
      const secondSegment = pathSegments[1];
      const thirdSegment = pathSegments[2];
      const fourthSegment = pathSegments[3];

      // Check if first segment is a compartment type
      const compartmentTypes = [
        "Patient",
        "Encounter",
        "RelatedPerson",
        "Practitioner",
        "Device",
      ];
      if (compartmentTypes.includes(firstSegment)) {
        // Compartment search: /[compartment]/[id]/* or /[compartment]/[id]/[type]
        if (
          thirdSegment === "*" ||
          (thirdSegment &&
            !thirdSegment.startsWith("_") &&
            !thirdSegment.startsWith("$"))
        ) {
          return {
            path: pathPart,
            resourceType: thirdSegment === "*" ? null : thirdSegment,
            id: null,
            compartmentType: firstSegment,
            compartmentId: secondSegment,
            operation: null,
            isHistory: false,
            isVHistory: false,
            isSearch: fourthSegment === "_search",
            isMetadata: false,
            isSystem: false,
            params,
          };
        }
        // Compartment search: /[compartment]/[id]/_search
        if (thirdSegment === "_search") {
          return {
            path: pathPart,
            resourceType: null,
            id: null,
            compartmentType: firstSegment,
            compartmentId: secondSegment,
            operation: null,
            isHistory: false,
            isVHistory: false,
            isSearch: true,
            isMetadata: false,
            isSystem: false,
            params,
          };
        }
      }
    }

    // Type-level or instance-level paths
    const resourceType = pathSegments[0];
    const id = pathSegments[1];
    const thirdSegment = pathSegments[2];
    const fourthSegment = pathSegments[3];

    // Instance-level history version: /[type]/[id]/_history/[vid]
    if (
      pathSegments.length === 4 &&
      thirdSegment === "_history" &&
      fourthSegment
    ) {
      return {
        path: pathPart,
        resourceType,
        id,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: true,
        isVHistory: true,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Instance-level history: /[type]/[id]/_history
    if (pathSegments.length === 3 && thirdSegment === "_history") {
      return {
        path: pathPart,
        resourceType,
        id,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: true,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Instance-level operation: /[type]/[id]/$operation
    if (pathSegments.length === 3 && thirdSegment?.startsWith("$")) {
      return {
        path: pathPart,
        resourceType,
        id,
        compartmentType: null,
        compartmentId: null,
        operation: thirdSegment.substring(1),
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Instance-level: /[type]/[id]
    if (pathSegments.length === 2 && id) {
      return {
        path: pathPart,
        resourceType,
        id,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Type-level search: /[type]/_search
    if (pathSegments.length === 2 && pathSegments[1] === "_search") {
      return {
        path: pathPart,
        resourceType,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: false,
        isVHistory: false,
        isSearch: true,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Type-level history: /[type]/_history
    if (pathSegments.length === 2 && pathSegments[1] === "_history") {
      return {
        path: pathPart,
        resourceType,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: null,
        isHistory: true,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Type-level operation: /[type]/$operation
    if (pathSegments.length === 2 && pathSegments[1]?.startsWith("$")) {
      return {
        path: pathPart,
        resourceType,
        id: null,
        compartmentType: null,
        compartmentId: null,
        operation: pathSegments[1].substring(1),
        isHistory: false,
        isVHistory: false,
        isSearch: false,
        isMetadata: false,
        isSystem: false,
        params,
      };
    }

    // Type-level: /[type] (may have query params)
    return {
      path: pathPart,
      resourceType,
      id: null,
      compartmentType: null,
      compartmentId: null,
      operation: null,
      isHistory: false,
      isVHistory: false,
      isSearch: false,
      isMetadata: false,
      isSystem: false,
      params,
    };
  };

  const parsed = parseQuery(inputValue);
  const { resourceType, params } = parsed;

  const scrollToSelectedItem = (index: number) => {
    const allItems =
      commandListRef.current?.querySelectorAll("[data-item-index]");
    const selectedElement = allItems?.[index];
    selectedElement?.scrollIntoView({
      behavior: "smooth",
      block: "nearest",
      inline: "nearest",
    });
  };

  const suggestions = useMemo(() => {
    const trimmed = inputValue.trim();
    const pathSegments = trimmed.split("/").filter((s) => s.length > 0);
    const lastSegment = pathSegments[pathSegments.length - 1] || "";
    const hasQueryParams = trimmed.includes("?");

    // Empty input - suggest system-level endpoints and resource types
    if (!trimmed) {
      const systemItems = [
        { id: "system-metadata", value: "metadata", type: "system" as const },
        { id: "system-search", value: "_search", type: "system" as const },
        { id: "system-history", value: "_history", type: "system" as const },
        { id: "system-operation", value: "$", type: "system" as const },
      ];
      const resourceItems = availableResources
        .map((r) => r.type)
        .filter(Boolean)
        .slice(0, 6)
        .map((type: string, index: number) => ({
          id: `resource-${index}`,
          value: type,
          type: "resource" as const,
        }));

      return {
        type: "mixed" as const,
        items: [...systemItems, ...resourceItems],
      };
    }

    // System-level suggestions (starting with /)
    if (
      trimmed === "/" ||
      (pathSegments.length === 0 && trimmed.startsWith("/"))
    ) {
      const items = [
        { id: "system-metadata", value: "metadata", type: "system" as const },
        { id: "system-search", value: "_search", type: "system" as const },
        { id: "system-history", value: "_history", type: "system" as const },
        { id: "system-operation", value: "$", type: "system" as const },
        ...availableResources
          .map((r) => r.type)
          .filter(Boolean)
          .slice(0, 6)
          .map((type: string, index: number) => ({
            id: `resource-${index}`,
            value: type,
            type: "resource" as const,
          })),
      ];
      return { type: "mixed" as const, items };
    }

    // Typing system-level endpoint
    if (pathSegments.length === 0 && trimmed.startsWith("/")) {
      const prefix = trimmed.substring(1).toLowerCase();
      const systemItems = [
        { id: "system-metadata", value: "metadata", type: "system" as const },
        { id: "system-search", value: "_search", type: "system" as const },
        { id: "system-history", value: "_history", type: "system" as const },
        { id: "system-operation", value: "$", type: "system" as const },
      ].filter((item) => item.value.toLowerCase().startsWith(prefix));
      const resourceItems = availableResources
        .map((r) => r.type)
        .filter(Boolean)
        .filter((type: string) => type.toLowerCase().startsWith(prefix))
        .slice(0, 6)
        .map((type: string, index: number) => ({
          id: `resource-${index}`,
          value: type,
          type: "resource" as const,
        }));

      return {
        type: "mixed" as const,
        items: [...systemItems, ...resourceItems],
      };
    }

    // No separator - suggest resource types or system endpoints
    if (!hasQueryParams && !trimmed.includes("/")) {
      const filtered = availableResources
        .map((r) => r.type)
        .filter(Boolean)
        .filter((type: string) =>
          type.toLowerCase().includes(trimmed.toLowerCase())
        )
        .slice(0, 10)
        .map((type: string, index: number) => ({
          id: `resource-${index}`,
          value: type,
          type: "resource" as const,
        }));

      const endsWithResourceType = availableResources.some(
        (r) => r.type === trimmed
      );

      if (endsWithResourceType) {
        const items = [
          { id: "separator-query", value: "?", type: "separator" as const },
          { id: "separator-slash", value: "/", type: "separator" as const },
        ];
        return {
          type: "separators" as const,
          items,
          resourceType: trimmed,
        };
      }

      return { type: "resources" as const, items: filtered };
    }

    // Handle path-based suggestions
    if (!hasQueryParams) {
      // System-level operation: /$[typing]
      if (pathSegments.length === 1 && pathSegments[0]?.startsWith("$")) {
        const operationPrefix = pathSegments[0].substring(1);
        const commonOperations = [
          "everything",
          "expand",
          "validate",
          "meta",
          "meta-add",
          "meta-delete",
        ];
        const items = commonOperations
          .filter((op) =>
            op.toLowerCase().startsWith(operationPrefix.toLowerCase())
          )
          .map((op, index) => ({
            id: `operation-${index}`,
            value: `$${op}`,
            type: "operation" as const,
          }));
        return { type: "operations" as const, items };
      }

      // Type-level: /[type] - suggest next steps
      if (pathSegments.length === 1 && parsed.resourceType) {
        const items = [
          { id: "type-query", value: "?", type: "separator" as const },
          { id: "type-slash", value: "/", type: "separator" as const },
          { id: "type-search", value: "_search", type: "path" as const },
          { id: "type-history", value: "_history", type: "path" as const },
          { id: "type-operation", value: "$", type: "path" as const },
        ];
        return {
          type: "type-actions" as const,
          items,
          resourceType: parsed.resourceType,
        };
      }

      // Instance-level: /[type]/[id] - suggest next steps
      if (pathSegments.length === 2 && parsed.resourceType && parsed.id) {
        const items = [
          { id: "instance-history", value: "_history", type: "path" as const },
          { id: "instance-operation", value: "$", type: "path" as const },
        ];
        return {
          type: "instance-actions" as const,
          items,
          resourceType: parsed.resourceType,
          id: parsed.id,
        };
      }

      // Instance-level typing ID: /[type]/[typing]
      if (pathSegments.length === 2 && parsed.resourceType && !parsed.id) {
        return {
          type: "instance" as const,
          items: [
            { id: "instance-0", value: "[id]", type: "instance" as const },
          ],
          resourceType: parsed.resourceType,
        };
      }

      // Type-level operation: /[type]/$[typing]
      if (
        pathSegments.length === 2 &&
        parsed.resourceType &&
        pathSegments[1]?.startsWith("$")
      ) {
        const operationPrefix = pathSegments[1].substring(1);
        const commonOperations = [
          "everything",
          "expand",
          "validate",
          "meta",
          "meta-add",
          "meta-delete",
        ];
        const items = commonOperations
          .filter((op) =>
            op.toLowerCase().startsWith(operationPrefix.toLowerCase())
          )
          .map((op, index) => ({
            id: `operation-${index}`,
            value: `$${op}`,
            type: "operation" as const,
          }));
        return {
          type: "operations" as const,
          items,
          resourceType: parsed.resourceType,
        };
      }

      // Instance-level operation: /[type]/[id]/$[typing]
      if (
        pathSegments.length === 3 &&
        parsed.resourceType &&
        parsed.id &&
        pathSegments[2]?.startsWith("$")
      ) {
        const operationPrefix = pathSegments[2].substring(1);
        const commonOperations = [
          "everything",
          "expand",
          "validate",
          "meta",
          "meta-add",
          "meta-delete",
        ];
        const items = commonOperations
          .filter((op) =>
            op.toLowerCase().startsWith(operationPrefix.toLowerCase())
          )
          .map((op, index) => ({
            id: `operation-${index}`,
            value: `$${op}`,
            type: "operation" as const,
          }));
        return {
          type: "operations" as const,
          items,
          resourceType: parsed.resourceType,
          id: parsed.id,
        };
      }

      // Instance-level history: /[type]/[id]/_history - suggest version ID
      if (
        pathSegments.length === 3 &&
        parsed.resourceType &&
        parsed.id &&
        pathSegments[2] === "_history"
      ) {
        return {
          type: "version" as const,
          items: [
            { id: "version-0", value: "[vid]", type: "version" as const },
          ],
          resourceType: parsed.resourceType,
          id: parsed.id,
        };
      }

      // Compartment: /[compartment]/[id] - suggest next steps
      if (
        pathSegments.length === 2 &&
        parsed.compartmentType &&
        parsed.compartmentId
      ) {
        const items = [
          { id: "compartment-all", value: "*", type: "path" as const },
          { id: "compartment-search", value: "_search", type: "path" as const },
          ...availableResources
            .map((r) => r.type)
            .filter(Boolean)
            .slice(0, 5)
            .map((type: string, index: number) => ({
              id: `compartment-resource-${index}`,
              value: type,
              type: "resource" as const,
            })),
        ];
        return {
          type: "compartment-actions" as const,
          items,
          compartmentType: parsed.compartmentType,
          compartmentId: parsed.compartmentId,
        };
      }

      // Compartment typing ID: /[compartment]/[typing]
      if (
        pathSegments.length === 2 &&
        parsed.compartmentType &&
        !parsed.compartmentId
      ) {
        return {
          type: "instance" as const,
          items: [
            {
              id: "compartment-id-0",
              value: "[id]",
              type: "instance" as const,
            },
          ],
          resourceType: parsed.compartmentType,
        };
      }
    }

    // Handle query parameters (search)
    if (hasQueryParams && parsed.resourceType) {
      const combinedSearchParams = getCombinedSearchParams(parsed.resourceType);
      if (combinedSearchParams.length === 0)
        return { type: "none" as const, items: [] };

      const lastParam = params[params.length - 1] || "";

      if (lastParam.includes("=")) {
        const paramName = lastParam.split("=")[0];
        const baseParamName = paramName.split(":")[0];
        const param = combinedSearchParams.find(
          (sp) => sp.name === baseParamName
        );

        if (param) {
          const applicableModifiers = SEARCH_MODIFIERS.filter((modifier) => {
            const type = param.type;
            if (type === "string")
              return [":exact", ":contains", ":not", ":missing"].includes(
                modifier
              );
            if (type === "token")
              return [
                ":not",
                ":missing",
                ":text",
                ":in",
                ":not-in",
                ":below",
                ":above",
                ":type",
              ].includes(modifier);
            if (type === "reference")
              return [":not", ":missing", ":type"].includes(modifier);
            return [":not", ":missing"].includes(modifier);
          });

          if (applicableModifiers.length > 0) {
            const items = applicableModifiers.map((mod, index) => ({
              id: `modifier-${index}`,
              value: `${baseParamName}${mod}`,
              type: "modifier" as const,
            }));
            return {
              type: "modifiers" as const,
              items,
              resourceType: parsed.resourceType,
              currentParam: baseParamName,
            };
          }
        }
        return { type: "none" as const, items: [] };
      }

      const isTypingParam = lastParam && !lastParam.includes("=");

      if (isTypingParam) {
        const prefix = SEARCH_PREFIXES.find((p) => lastParam.startsWith(p));

        if (prefix) {
          const paramPart = lastParam.substring(prefix.length);
          const filtered = combinedSearchParams
            .filter(
              (sp) =>
                sp.name &&
                sp.name.toLowerCase().includes(paramPart.toLowerCase())
            )
            .map((sp) => ({
              name: sp.name!,
              category: (sp as any).category || "resource",
            }));

          const items = filtered.map((item, index) => ({
            id: `parameter-prefix-${index}`,
            value: `${prefix}${item.name}`,
            type: "parameter" as const,
            category: item.category,
          }));
          return {
            type: "parameters" as const,
            items,
            resourceType: parsed.resourceType,
            hasPrefix: true,
          };
        }

        const paramFiltered = combinedSearchParams
          .filter(
            (sp) =>
              sp.name && sp.name.toLowerCase().includes(lastParam.toLowerCase())
          )
          .map((sp) => ({
            name: sp.name!,
            category: (sp as any).category || "resource",
          }));

        const prefixFiltered = SEARCH_PREFIXES.filter((p) =>
          p.toLowerCase().includes(lastParam.toLowerCase())
        );

        const items = [
          ...paramFiltered.map((item, index) => ({
            id: `parameter-${index}`,
            value: item.name,
            type: "parameter" as const,
            category: item.category,
          })),
          ...prefixFiltered.map((item, index) => ({
            id: `prefix-${index}`,
            value: item,
            type: "prefix" as const,
          })),
        ];
        return {
          type: "parameters" as const,
          items,
          resourceType: parsed.resourceType,
        };
      }

      const allParams = combinedSearchParams
        .filter((sp) => sp.name)
        .map((sp) => ({
          name: sp.name!,
          category: (sp as any).category || "resource",
        }));
      const items = [
        ...allParams.map((item, index) => ({
          id: `all-parameter-${index}`,
          value: item.name,
          type: "parameter" as const,
          category: item.category,
        })),
        ...SEARCH_PREFIXES.map((item, index) => ({
          id: `all-prefix-${index}`,
          value: item,
          type: "prefix" as const,
        })),
      ];
      return {
        type: "parameters" as const,
        items,
        resourceType: parsed.resourceType,
      };
    }

    return { type: "none" as const, items: [] };
  }, [inputValue, availableResources, getCombinedSearchParams, parsed, params]);

  const handleSuggestionSelect = (suggestion: {
    value: string;
    type?: string;
  }) => {
    const { value: suggestionValue } = suggestion;
    const currentPath = inputValue.split("?")[0];
    const currentQuery = inputValue.includes("?")
      ? inputValue.split("?")[1]
      : "";

    // Mixed suggestions (system endpoints + resources)
    if (suggestions.type === "mixed") {
      if (suggestion.type === "system") {
        const newQuery = `/${suggestionValue}`;
        setInputValue(newQuery);
        setSearchQuery(newQuery);
      } else {
        setInputValue(suggestionValue);
        setSearchQuery(suggestionValue);
      }
      return;
    }

    // Resources
    if (suggestions.type === "resources") {
      setInputValue(suggestionValue);
      setSearchQuery(suggestionValue);
      return;
    }

    // Separators
    if (suggestions.type === "separators") {
      const newQuery = inputValue + suggestionValue;
      setInputValue(newQuery);
      setSearchQuery(newQuery);
      return;
    }

    // Instance ID placeholder
    if (suggestions.type === "instance") {
      setOpen(false);
      setSelectedIndex(-1);
      setSelectedValue(null);
      return;
    }

    // Version ID placeholder
    if (suggestions.type === "version") {
      setOpen(false);
      setSelectedIndex(-1);
      setSelectedValue(null);
      return;
    }

    // Type-level actions
    if (suggestions.type === "type-actions") {
      if (suggestionValue === "?") {
        const newQuery = `${currentPath}?`;
        setInputValue(newQuery);
        setSearchQuery(newQuery);
      } else if (suggestionValue === "/") {
        const newQuery = `${currentPath}/`;
        setInputValue(newQuery);
        setSearchQuery(newQuery);
      } else {
        const newQuery = `${currentPath}/${suggestionValue}`;
        setInputValue(newQuery);
        setSearchQuery(newQuery);
      }
      return;
    }

    // Instance-level actions
    if (suggestions.type === "instance-actions") {
      const newQuery = `${currentPath}/${suggestionValue}`;
      setInputValue(newQuery);
      setSearchQuery(newQuery);
      return;
    }

    // Compartment actions
    if (suggestions.type === "compartment-actions") {
      if (suggestion.type === "resource") {
        const newQuery = `${currentPath}/${suggestionValue}`;
        setInputValue(newQuery);
        setSearchQuery(newQuery);
      } else {
        const newQuery = `${currentPath}/${suggestionValue}`;
        setInputValue(newQuery);
        setSearchQuery(newQuery);
      }
      return;
    }

    // Operations
    if (suggestions.type === "operations") {
      const newQuery = `${currentPath}/${suggestionValue}`;
      setInputValue(newQuery);
      setSearchQuery(newQuery);
      return;
    }

    // Modifiers
    if (suggestions.type === "modifiers") {
      const baseQuery = `${parsed.resourceType}?`;
      const existingParams = params.slice(0, -1);
      const currentParam = params[params.length - 1];
      const paramValue = currentParam.split("=")[1] || "";
      const newParam = `${suggestionValue}=${paramValue}`;
      const paramString =
        existingParams.length > 0
          ? existingParams.join("&") + "&" + newParam
          : newParam;
      const newQuery = baseQuery + paramString;
      setInputValue(newQuery);
      setSearchQuery(newQuery);
      return;
    }

    // Parameters
    if (suggestions.type === "parameters") {
      const baseQuery = `${parsed.resourceType}?`;
      const existingParams = params.slice(0, -1);
      const isPrefix = SEARCH_PREFIXES.includes(
        suggestionValue as (typeof SEARCH_PREFIXES)[number]
      );

      const newParam = isPrefix ? suggestionValue : `${suggestionValue}=`;
      const paramString =
        existingParams.length > 0
          ? existingParams.join("&") + "&" + newParam
          : newParam;
      const newQuery = baseQuery + paramString;
      setInputValue(newQuery);
      setSearchQuery(newQuery);
      return;
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setInputValue(e.target.value);
    setSearchQuery(e.target.value);
    setSelectedIndex(-1);
    setSelectedValue(null);
    setOpen(true);
  };

  const updateSelectedIndex = (direction: "up" | "down") => {
    setSelectedIndex((prev) => {
      const newIndex =
        direction === "down"
          ? prev < suggestions.items.length - 1
            ? prev + 1
            : 0
          : prev > 0
          ? prev - 1
          : suggestions.items.length - 1;

      const newItem = suggestions.items[newIndex];
      if (newItem) {
        const prefix =
          suggestions.type === "resources"
            ? "resource"
            : suggestions.type === "separators"
            ? "separator"
            : suggestions.type === "instance"
            ? "instance"
            : suggestions.type === "version"
            ? "version"
            : suggestions.type === "modifiers"
            ? "modifier"
            : suggestions.type === "mixed"
            ? newItem.type === "system"
              ? "system"
              : "resource"
            : suggestions.type === "type-actions"
            ? "type-action"
            : suggestions.type === "instance-actions"
            ? "instance-action"
            : suggestions.type === "compartment-actions"
            ? "compartment-action"
            : suggestions.type === "operations"
            ? "operation"
            : "parameter";
        setSelectedValue(`${prefix}-${newItem.value}`);
      }

      setTimeout(() => scrollToSelectedItem(newIndex), 0);
      return newIndex;
    });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      if (open && selectedIndex >= 0 && suggestions.items[selectedIndex]) {
        e.preventDefault();
        handleSuggestionSelect(suggestions.items[selectedIndex]);
      } else if (open) {
        setOpen(false);
        setSelectedIndex(-1);
        setSelectedValue(null);
      }
      return;
    }

    if (!open) return;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      updateSelectedIndex("down");
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      updateSelectedIndex("up");
    } else if (e.key === "Escape") {
      setOpen(false);
      setSelectedIndex(-1);
      setSelectedValue(null);
    }
  };

  useEffect(() => {
    setInputValue(searchQuery);
  }, [searchQuery]);

  useEffect(() => {
    setSelectedIndex(-1);
    setSelectedValue(null);
  }, [suggestions.items.length, suggestions.type]);

  return (
    <div className="relative flex-1">
      <Input
        type="text"
        value={inputValue}
        onChange={handleInputChange}
        onKeyDown={handleKeyDown}
        onFocus={() => setOpen(true)}
        onBlur={() => {
          setTimeout(() => {
            if (!popoverRef.current?.contains(document.activeElement)) {
              setOpen(false);
            }
          }, 150);
        }}
        placeholder="FHIR Endpoint..."
        className={cn("pr-10 font-mono", inputClassName)}
      />
      {inputValue && (
        <button
          type="button"
          onClick={() => {
            setInputValue("");
            setSearchQuery("");
            setSelectedIndex(-1);
            setSelectedValue(null);
          }}
          className="absolute right-3 top-1/2 -translate-y-1/2 p-1 hover:bg-muted rounded-sm transition-colors z-10"
          title="Clear search"
        >
          <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
        </button>
      )}
      {open && (
        <div
          ref={popoverRef}
          className="absolute top-full left-0 right-0 z-50 mt-1 rounded-md border bg-popover text-popover-foreground shadow-md"
        >
          <Command>
            <CommandList
              ref={commandListRef}
              className="max-h-72 overflow-auto"
            >
              <CommandEmpty>No suggestions found.</CommandEmpty>

              {suggestions.type === "resources" && (
                <CommandGroup heading="Resource Types">
                  {suggestions.items.map((resource, index) => (
                    <CommandItem
                      key={resource.id}
                      value={`resource-${resource.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(resource)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `resource-${resource.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      <Database className="h-4 w-4" />
                      {resource.value}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "separators" && (
                <CommandGroup heading="Next Step">
                  {suggestions.items.map((separator, index) => (
                    <CommandItem
                      key={separator.id}
                      value={`separator-${separator.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(separator)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `separator-${separator.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      {separator.value === "?" ? (
                        <>
                          <HelpCircle className="h-4 w-4" />? - Search
                          parameters
                        </>
                      ) : (
                        <>
                          <Slash className="h-4 w-4" />/ - Resource instance
                        </>
                      )}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "instance" && (
                <CommandGroup heading="Instance ID">
                  {suggestions.items.map((placeholder, index) => (
                    <CommandItem
                      key={placeholder.id}
                      value={`instance-${placeholder.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(placeholder)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `instance-${placeholder.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      <Hash className="h-4 w-4" />
                      {placeholder.value}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "parameters" && (
                <CommandGroup
                  heading={`Search Parameters for ${suggestions.resourceType}`}
                >
                  {suggestions.items.map((param, index) => {
                    const isPrefix = param.type === "prefix";
                    const category = (param as any).category;
                    return (
                      <CommandItem
                        key={param.id}
                        value={`parameter-${param.value}`}
                        data-item-index={index}
                        onSelect={() => handleSuggestionSelect(param)}
                        className={cn(
                          "flex items-center gap-2 cursor-pointer",
                          selectedValue === `parameter-${param.value}` &&
                            "bg-accent text-accent-foreground"
                        )}
                      >
                        {isPrefix ? (
                          <>
                            <Settings className="h-4 w-4" />
                            <span className="text-orange-600 dark:text-orange-400">
                              {param.value}
                            </span>
                            <span className="text-xs text-muted-foreground ml-auto">
                              prefix
                            </span>
                          </>
                        ) : (
                          <>
                            <Filter className="h-4 w-4" />
                            {param.value}
                            {category && (
                              <span className="text-xs text-muted-foreground ml-auto">
                                {category}
                              </span>
                            )}
                          </>
                        )}
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              )}

              {suggestions.type === "modifiers" && (
                <CommandGroup
                  heading={`Modifiers for ${suggestions.currentParam}`}
                >
                  {suggestions.items.map((modifier, index) => (
                    <CommandItem
                      key={modifier.id}
                      value={`modifier-${modifier.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(modifier)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `modifier-${modifier.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      <Settings className="h-4 w-4" />
                      <span className="text-blue-600 dark:text-blue-400">
                        {modifier.value}
                      </span>
                      <span className="text-xs text-muted-foreground ml-auto">
                        modifier
                      </span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "mixed" && (
                <>
                  <CommandGroup heading="System Endpoints">
                    {suggestions.items
                      .filter((item) => item.type === "system")
                      .map((item, index) => (
                        <CommandItem
                          key={item.id}
                          value={`system-${item.value}`}
                          data-item-index={suggestions.items.indexOf(item)}
                          onSelect={() => handleSuggestionSelect(item)}
                          className={cn(
                            "flex items-center gap-2 cursor-pointer",
                            selectedValue === `system-${item.value}` &&
                              "bg-accent text-accent-foreground"
                          )}
                        >
                          <Settings className="h-4 w-4" />
                          {item.value}
                        </CommandItem>
                      ))}
                  </CommandGroup>
                  <CommandGroup heading="Resource Types">
                    {suggestions.items
                      .filter((item) => item.type === "resource")
                      .map((item, index) => (
                        <CommandItem
                          key={item.id}
                          value={`resource-${item.value}`}
                          data-item-index={suggestions.items.indexOf(item)}
                          onSelect={() => handleSuggestionSelect(item)}
                          className={cn(
                            "flex items-center gap-2 cursor-pointer",
                            selectedValue === `resource-${item.value}` &&
                              "bg-accent text-accent-foreground"
                          )}
                        >
                          <Database className="h-4 w-4" />
                          {item.value}
                        </CommandItem>
                      ))}
                  </CommandGroup>
                </>
              )}

              {suggestions.type === "type-actions" && (
                <CommandGroup
                  heading={`Actions for ${suggestions.resourceType}`}
                >
                  {suggestions.items.map((item, index) => (
                    <CommandItem
                      key={item.id}
                      value={`type-action-${item.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(item)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `type-action-${item.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      {item.value === "?" ? (
                        <>
                          <HelpCircle className="h-4 w-4" />? - Search
                          parameters
                        </>
                      ) : item.value === "/" ? (
                        <>
                          <Slash className="h-4 w-4" />/ - Resource instance
                        </>
                      ) : item.value === "_search" ? (
                        <>
                          <Filter className="h-4 w-4" />
                          _search - POST search
                        </>
                      ) : item.value === "_history" ? (
                        <>
                          <Hash className="h-4 w-4" />
                          _history - Resource history
                        </>
                      ) : (
                        <>
                          <Settings className="h-4 w-4" />$ - Operation
                        </>
                      )}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "instance-actions" && (
                <CommandGroup
                  heading={`Actions for ${suggestions.resourceType}/${suggestions.id}`}
                >
                  {suggestions.items.map((item, index) => (
                    <CommandItem
                      key={item.id}
                      value={`instance-action-${item.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(item)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `instance-action-${item.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      {item.value === "_history" ? (
                        <>
                          <Hash className="h-4 w-4" />
                          _history - Version history
                        </>
                      ) : (
                        <>
                          <Settings className="h-4 w-4" />$ - Operation
                        </>
                      )}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "compartment-actions" && (
                <CommandGroup
                  heading={`Compartment: ${suggestions.compartmentType}/${suggestions.compartmentId}`}
                >
                  {suggestions.items.map((item, index) => (
                    <CommandItem
                      key={item.id}
                      value={`compartment-action-${item.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(item)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `compartment-action-${item.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      {item.value === "*" ? (
                        <>
                          <Database className="h-4 w-4" />* - All resources
                        </>
                      ) : item.value === "_search" ? (
                        <>
                          <Filter className="h-4 w-4" />
                          _search - POST search
                        </>
                      ) : (
                        <>
                          <Database className="h-4 w-4" />
                          {item.value}
                        </>
                      )}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "operations" && (
                <CommandGroup
                  heading={`Operations${
                    suggestions.resourceType
                      ? ` for ${suggestions.resourceType}${
                          suggestions.id ? `/${suggestions.id}` : ""
                        }`
                      : ""
                  }`}
                >
                  {suggestions.items.map((item, index) => (
                    <CommandItem
                      key={item.id}
                      value={`operation-${item.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(item)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `operation-${item.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      <Settings className="h-4 w-4" />
                      <span className="text-purple-600 dark:text-purple-400">
                        {item.value}
                      </span>
                      <span className="text-xs text-muted-foreground ml-auto">
                        operation
                      </span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {suggestions.type === "version" && (
                <CommandGroup heading="Version ID">
                  {suggestions.items.map((placeholder, index) => (
                    <CommandItem
                      key={placeholder.id}
                      value={`version-${placeholder.value}`}
                      data-item-index={index}
                      onSelect={() => handleSuggestionSelect(placeholder)}
                      className={cn(
                        "flex items-center gap-2 cursor-pointer",
                        selectedValue === `version-${placeholder.value}` &&
                          "bg-accent text-accent-foreground"
                      )}
                    >
                      <Hash className="h-4 w-4" />
                      {placeholder.value}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}
            </CommandList>
          </Command>
        </div>
      )}
    </div>
  );
}
