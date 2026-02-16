import { useMemo } from "react";
import { CapabilityStatement } from "fhir/r4";
import { Button } from "@thalamiq/ui/components/button";
import { Loader2, PlayIcon, Send } from "lucide-react";
import FhirSearchInput from "./FhirSearchInput";
import SearchParams from "./SearchParams";

interface FhirInputProps {
  searchQuery: string;
  setSearchQuery: (searchQuery: string) => void;
  handleSearch: (e: React.FormEvent<Element>) => void;
  resourceType: string | null;
  loading: boolean;
  capabilityStatement?: CapabilityStatement;
  actionButtons?: React.ReactNode;
}

// Common Parameters defined for all resources
const COMMON_PARAMS = [
  {
    name: "_content",
    type: "string" as const,
    documentation: "Text search against the entire resource",
  },
  {
    name: "_filter",
    type: "special" as const,
    documentation:
      "Filter search parameter which supports a more sophisticated grammar for searching",
  },
  {
    name: "_has",
    type: "special" as const,
    documentation: "Provides limited support for reverse chaining",
  },
  {
    name: "_id",
    type: "token" as const,
    documentation: "Resource id (not a full URL)",
  },
  {
    name: "_in",
    type: "reference" as const,
    documentation: "Group, List, or CareTeam membership",
  },
  {
    name: "_language",
    type: "token" as const,
    documentation: "Language of the resource content",
  },
  {
    name: "_lastUpdated",
    type: "date" as const,
    documentation:
      "Date last updated. Server has discretion on the boundary precision",
  },
  {
    name: "_list",
    type: "string" as const,
    documentation: "All resources in nominated list (by id, not a full URL)",
  },
  {
    name: "_profile",
    type: "reference" as const,
    documentation: "Search for all resources tagged with a profile",
  },
  {
    name: "_query",
    type: "string" as const,
    documentation: "Custom named query",
  },
  {
    name: "_security",
    type: "token" as const,
    documentation: "Search by a security label",
  },
  {
    name: "_source",
    type: "uri" as const,
    documentation: "Search by where the resource comes from",
  },
  {
    name: "_tag",
    type: "token" as const,
    documentation: "Search by a resource tag",
  },
  {
    name: "_text",
    type: "string" as const,
    documentation: "Text search against the narrative",
  },
];

// Search Control Parameters
const SEARCH_CONTROL_PARAMS = [
  {
    name: "_contained",
    type: "string" as const,
    documentation:
      "Whether to return resources contained in other resources in the search matches (true | false | both)",
  },
  {
    name: "_containedType",
    type: "string" as const,
    documentation:
      "If returning contained resources, whether to return the contained or container resources (container | contained)",
  },
  {
    name: "_count",
    type: "number" as const,
    documentation: "Number of results per page",
  },
  {
    name: "_elements",
    type: "token" as const,
    documentation:
      "Request that only a specific set of elements be returned for resources",
  },
  {
    name: "_graph",
    type: "reference" as const,
    documentation: "Include related resources according to a GraphDefinition",
  },
  {
    name: "_include",
    type: "string" as const,
    documentation:
      "Other resources to include in the search results that search matches point to",
  },
  {
    name: "_maxresults",
    type: "number" as const,
    documentation:
      "Hint to a server that only the first 'n' results will ever be processed",
  },
  {
    name: "_revinclude",
    type: "string" as const,
    documentation:
      "Other resources to include in the search results when they refer to search matches",
  },
  {
    name: "_score",
    type: "token" as const,
    documentation: "Request match relevance in results (true | false)",
  },
  {
    name: "_sort",
    type: "string" as const,
    documentation:
      "Order to sort results in (can repeat for inner sort orders)",
  },
  {
    name: "_summary",
    type: "string" as const,
    documentation:
      "Just return the summary elements (for resources where this is defined) (true | false)",
  },
  {
    name: "_total",
    type: "token" as const,
    documentation:
      "Request a precision of the total number of results for a request (none | estimate | accurate)",
  },
];

export default function FhirInput({
  searchQuery,
  setSearchQuery,
  loading,
  handleSearch,
  resourceType,
  capabilityStatement,
  actionButtons,
}: FhirInputProps) {
  const availableSearchParams = useMemo(() => {
    const restConfig = capabilityStatement?.rest?.[0];
    const resource = resourceType
      ? restConfig?.resource?.find((r) => r.type === resourceType)
      : null;
    const globalSearchParams = restConfig?.searchParam || [];
    const resourceParams = resource?.searchParam || [];

    // Combine resource-specific and global params
    const combined = [...resourceParams, ...globalSearchParams];
    const paramNames = new Set(combined.map((p) => p.name));

    // Add common params if they're not already included
    const commonParamsToAdd = COMMON_PARAMS.filter(
      (p) => !paramNames.has(p.name)
    ).map((p) => ({
      name: p.name,
      type: p.type,
      documentation: p.documentation,
    }));

    // Add search control params if they're not already included
    const controlParamsToAdd = SEARCH_CONTROL_PARAMS.filter(
      (p) => !paramNames.has(p.name)
    ).map((p) => ({
      name: p.name,
      type: p.type,
      documentation: p.documentation,
    }));

    // Create a map of common params for quick lookup
    const commonParamsMap = new Map(COMMON_PARAMS.map((p) => [p.name, p]));
    const controlParamsMap = new Map(
      SEARCH_CONTROL_PARAMS.map((p) => [p.name, p])
    );

    // Separate into categories, ensuring documentation is preserved
    const commonParamNames = new Set(COMMON_PARAMS.map((p) => p.name));
    const controlParamNames = new Set(SEARCH_CONTROL_PARAMS.map((p) => p.name));

    const commonParams = combined
      .filter((p) => commonParamNames.has(p.name))
      .map((p) => ({
        ...p,
        documentation:
          p.documentation || commonParamsMap.get(p.name)?.documentation,
      }))
      .concat(commonParamsToAdd);

    const controlParams = combined
      .filter((p) => controlParamNames.has(p.name))
      .map((p) => ({
        ...p,
        documentation:
          p.documentation || controlParamsMap.get(p.name)?.documentation,
      }))
      .concat(controlParamsToAdd);

    const resourceSpecificParams = combined.filter(
      (p) => !commonParamNames.has(p.name) && !controlParamNames.has(p.name)
    );

    return {
      resourceSpecific: resourceSpecificParams,
      common: commonParams,
      control: controlParams,
    };
  }, [capabilityStatement, resourceType]);

  const handleParameterClick = (paramName: string) => {
    const currentQuery = searchQuery.trim();
    let newQuery: string;

    if (!currentQuery) {
      newQuery = resourceType
        ? `${resourceType}?${paramName}=`
        : `${paramName}=`;
    } else if (currentQuery.includes("?")) {
      newQuery = `${currentQuery}&${paramName}=`;
    } else {
      newQuery = `${currentQuery}?${paramName}=`;
    }

    setSearchQuery(newQuery);
  };

  const handleFormSubmit = (e: React.FormEvent<Element>) => {
    e.preventDefault();
    handleSearch(e);
  };

  return (
    <div className="space-y-3">
      <form onSubmit={handleFormSubmit} className="flex gap-3">
        <FhirSearchInput
          searchQuery={searchQuery}
          setSearchQuery={setSearchQuery}
          inputClassName="h-10"
          capabilityStatement={capabilityStatement}
        />
        <Button
          type="submit"
          disabled={loading}
          size="icon"
          className="h-10 w-10"
        >
          {loading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Send className="h-4 w-4" />
          )}
        </Button>
      </form>

      <SearchParams
        groupedParams={{
          resourceSpecific: availableSearchParams.resourceSpecific
            .filter((p) => Boolean(p.name))
            .map((p) => ({
              name: p.name as string,
              documentation: p.documentation,
            })),
          common: availableSearchParams.common
            .filter((p) => Boolean(p.name))
            .map((p) => ({
              name: p.name as string,
              documentation: p.documentation,
            })),
          control: availableSearchParams.control
            .filter((p) => Boolean(p.name))
            .map((p) => ({
              name: p.name as string,
              documentation: p.documentation,
            })),
        }}
        onParamClick={handleParameterClick}
        actionButtons={actionButtons}
      />
    </div>
  );
}
