import React, { useState, useMemo, useCallback, memo } from "react";
import { Braces, ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@thalamiq/ui/utils";
import { detectValueType } from "@/lib/json";

interface JsonViewerProps {
  data: unknown;
  copyable?: boolean;
  downloadable?: boolean;
  theme?: "light" | "dark" | "auto";
  maxHeight?: string;
  className?: string;
  onDataChange?: (data: unknown) => void;
  maxSizeForTreeView?: number; // Max size in bytes before fallback to simple view
  maxSizeForHighlighting?: number; // Max size in bytes before disabling highlighting
}

// Memoized value renderer
const JsonValue = memo(({ value }: { value: unknown }) => {
  if (value === null) {
    return <span className="json-null">null</span>;
  }

  if (typeof value === "boolean") {
    return <span className="json-boolean">{value.toString()}</span>;
  }

  if (typeof value === "number") {
    return <span className="json-number">{value}</span>;
  }

  if (typeof value === "string") {
    const type = detectValueType(value);

    if (type === "date") {
      return (
        <span className="json-date">
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
          <span style={{ userSelect: "text" }}>{value}</span>
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
        </span>
      );
    }

    if (type === "url" && value.startsWith("http")) {
      return (
        <a
          href={value}
          target="_blank"
          rel="noopener noreferrer"
          className="json-url"
          title={value}
        >
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
          <span style={{ userSelect: "text" }}>{value}</span>
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
        </a>
      );
    }

    if (type === "reference") {
      return (
        <a
          href={`/requests?endpoint=${encodeURIComponent(value)}`}
          className="json-reference"
          title={`View ${value}`}
        >
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
          <span style={{ userSelect: "text" }}>{value}</span>
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
        </a>
      );
    }

    if (type === "uuid") {
      return (
        <span className="json-uuid">
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
          <span style={{ userSelect: "text" }}>{value}</span>
          <span className="json-quote" style={{ userSelect: "none" }}>
            &quot;
          </span>
        </span>
      );
    }

    return (
      <span className="json-string">
        <span className="json-quote" style={{ userSelect: "none" }}>
          &quot;
        </span>
        <span style={{ userSelect: "text" }}>{value}</span>
        <span className="json-quote" style={{ userSelect: "none" }}>
          &quot;
        </span>
      </span>
    );
  }

  return null;
});
JsonValue.displayName = "JsonValue";

// Memoized JSON node renderer
interface JsonNodeProps {
  data: unknown;
  keyName?: string;
  path: string;
  isLast: boolean;
  collapsedPaths: Set<string>;
  togglePath: (path: string) => void;
}

const JsonNode = memo(
  ({
    data,
    keyName,
    path,
    isLast,
    collapsedPaths,
    togglePath,
  }: JsonNodeProps) => {
    const isArray = Array.isArray(data);
    const isObject = data !== null && typeof data === "object" && !isArray;
    const isCollapsible = isArray || isObject;
    const isCollapsed = collapsedPaths.has(path);

    if (!isCollapsible) {
      return (
        <div className="json-line">
          {keyName && (
            <>
              <span className="json-key">&quot;{keyName}&quot;</span>
              <span className="json-separator">: </span>
            </>
          )}
          <span className="json-value-wrapper">
            <JsonValue value={data} />
          </span>
          {!isLast && <span className="json-separator">,</span>}
        </div>
      );
    }

    const entries = isArray
      ? data.map((item, idx) => [idx.toString(), item])
      : Object.entries(data);

    const handleToggle = (e: React.MouseEvent) => {
      e.stopPropagation();
      togglePath(path);
    };

    return (
      <div className="json-node">
        <div className="json-line">
          {isCollapsible && (
            <button
              onClick={handleToggle}
              className="json-toggle"
              aria-label={isCollapsed ? "Expand" : "Collapse"}
            >
              {isCollapsed ? (
                <ChevronRight className="w-3 h-3" />
              ) : (
                <ChevronDown className="w-3 h-3" />
              )}
            </button>
          )}
          {keyName && (
            <>
              <span className="json-key">&quot;{keyName}&quot;</span>
              <span className="json-separator">: </span>
            </>
          )}
          <span>{isArray ? "[" : "{"}</span>
          {isCollapsed && (
            <>
              <span className="json-collapsed">
                {" "}
                ... {entries.length} {isArray ? "items" : "properties"}{" "}
              </span>
              <span>{isArray ? "]" : "}"}</span>
              {!isLast && <span>,</span>}
            </>
          )}
        </div>

        {!isCollapsed && (
          <>
            <div className="json-children">
              {entries.map(([key, value], idx) => (
                <JsonNode
                  key={`${path}.${key}`}
                  data={value}
                  keyName={isArray ? undefined : key}
                  path={`${path}.${key}`}
                  isLast={idx === entries.length - 1}
                  collapsedPaths={collapsedPaths}
                  togglePath={togglePath}
                />
              ))}
            </div>
            <div className="json-line">
              <span>{isArray ? "]" : "}"}</span>
              {!isLast && <span className="json-separator">,</span>}
            </div>
          </>
        )}
      </div>
    );
  }
);
JsonNode.displayName = "JsonNode";

const JsonViewer = ({
  data,
  className,
  maxSizeForTreeView = 1000000, // Default 1MB
  maxSizeForHighlighting = 5000000, // Default 5MB
}: JsonViewerProps) => {
  // Memos
  const jsonString = useMemo(() => JSON.stringify(data, null, 2), [data]);

  const dataSize = useMemo(() => new Blob([jsonString]).size, [jsonString]);

  // Determine if data is too large for tree view
  const isTooLargeForTreeView = useMemo(
    () => dataSize > maxSizeForTreeView,
    [dataSize, maxSizeForTreeView]
  );

  // Determine if data is too large for highlighting
  const isTooLargeForHighlighting = useMemo(
    () => dataSize > maxSizeForHighlighting,
    [dataSize, maxSizeForHighlighting]
  );

  // State for line numbers and tree view
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());

  // Toggle collapse state for a path
  const togglePath = useCallback((path: string) => {
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  // FHIR-aware JSON syntax highlighting with clickable links
  const highlightedJson = useMemo(() => {
    let result = jsonString;

    // Patterns for FHIR-specific values
    const patterns = {
      // ISO 8601 date/datetime
      date: /^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?)?$/,
      // URLs and URIs (http, https, urn)
      url: /^(https?:\/\/|urn:)/,
      // FHIR references (ResourceType/id or just id)
      reference: /^[A-Z][a-zA-Z]+\/[a-zA-Z0-9\-\.]+$/,
      // UUIDs
      uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
    };

    // Match and highlight key-value pairs
    result = result.replace(
      /("(?:[^"\\]|\\.)*")\s*:\s*("(?:[^"\\]|\\.)*"|true|false|null|-?\d+\.?\d*(?:[eE][+-]?\d+)?|\[|\{)/g,
      (_match, key, value) => {
        let highlightedValue = value;

        // Highlight based on value type
        if (value.startsWith('"')) {
          const stringValue = value.slice(1, -1); // Remove quotes
          const escapedStringValue = stringValue.replace(/"/g, "&quot;");

          // Check for FHIR-specific patterns
          if (patterns.date.test(stringValue)) {
            highlightedValue = `<span class="json-date"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
          } else if (patterns.url.test(stringValue)) {
            // Make URLs clickable
            if (stringValue.startsWith("http")) {
              highlightedValue = `<a href="${stringValue}" target="_blank" rel="noopener noreferrer" class="json-url" title="${stringValue}"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></a>`;
            } else {
              highlightedValue = `<span class="json-url" title="${stringValue}"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
            }
          } else if (patterns.reference.test(stringValue)) {
            // Make FHIR references clickable (navigate to result page with query)
            highlightedValue = `<a href="/requests?endpoint=${encodeURIComponent(
              stringValue
            )}" class="json-reference" title="View ${stringValue}"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></a>`;
          } else if (patterns.uuid.test(stringValue)) {
            highlightedValue = `<span class="json-uuid"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
          } else {
            highlightedValue = `<span class="json-string"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
          }
        } else if (value === "true" || value === "false") {
          highlightedValue = `<span class="json-boolean">${value}</span>`;
        } else if (value === "null") {
          highlightedValue = `<span class="json-null">${value}</span>`;
        } else if (
          !isNaN(parseFloat(value)) &&
          value !== "[" &&
          value !== "{"
        ) {
          highlightedValue = `<span class="json-number">${value}</span>`;
        }

        return `<span class="json-key">${key}</span>: ${highlightedValue}`;
      }
    );

    return result;
  }, [jsonString]);

  if (!data) {
    return (
      <div className="flex items-center justify-center p-8 text-muted-foreground text-xs">
        <div className="text-center">
          <Braces className="w-8 h-8 mx-auto mb-2 opacity-50" />
          <p>No data to display</p>
        </div>
      </div>
    );
  }

  return (
    <div className={cn("bg-card max-w-full overflow-x-auto", className)}>
      {/* Content */}
      {!isTooLargeForTreeView ? (
        <div className="p-4 text-xs font-mono text-foreground json-tree-view overflow-x-auto">
          <JsonNode
            data={data}
            path="root"
            isLast={true}
            collapsedPaths={collapsedPaths}
            togglePath={togglePath}
          />
        </div>
      ) : isTooLargeForHighlighting ? (
        <div className="flex overflow-x-auto">
          <pre className="flex-1 text-xs font-mono text-foreground p-4 whitespace-pre-wrap wrap-break-word leading-5 max-w-full">
            <code className="json-syntax wrap-break-word">{jsonString}</code>
          </pre>
        </div>
      ) : (
        <div className="flex overflow-x-auto">
          <pre className="flex-1 text-xs font-mono text-foreground p-4 whitespace-pre-wrap wrap-break-word leading-5 max-w-full">
            <code
              dangerouslySetInnerHTML={{ __html: highlightedJson }}
              className="json-syntax wrap-break-word"
            />
          </pre>
        </div>
      )}

      {/* Inline styles for FHIR-aware syntax highlighting */}
      <style>{`
        /* Tree view styles */
        .json-tree-view {
          line-height: 1.6;
          overflow-wrap: break-word;
          word-break: break-word;
          max-width: 100%;
        }

        .json-node {
          display: block;
          max-width: 100%;
          overflow-wrap: break-word;
        }

        .json-line {
          display: flex;
          align-items: flex-start;
          flex-wrap: wrap;
          min-height: 1.6em;
          gap: 0.25rem;
        }

        .json-value-wrapper {
          display: inline;
          margin: 0;
          padding: 0;
          word-break: break-word;
          overflow-wrap: break-word;
          max-width: 100%;
        }

        .json-separator {
          user-select: none;
          -webkit-user-select: none;
          -moz-user-select: none;
          -ms-user-select: none;
        }

        .json-children {
          padding-left: 1.5rem;
          border-left: 1px solid hsl(var(--border));
          margin-left: 0.5rem;
          max-width: 100%;
          overflow-wrap: break-word;
        }

        .json-toggle {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          padding: 0;
          margin: 0;
          background: none;
          border: none;
          cursor: pointer;
          color: hsl(var(--muted-foreground));
          transition: color 0.15s ease;
          flex-shrink: 0;
        }

        .json-toggle:hover {
          color: hsl(var(--foreground));
        }

        .json-collapsed {
          color: hsl(var(--muted-foreground));
          font-style: italic;
          font-size: 0.95em;
        }

        /* Property keys - Soft blue */
        .json-key {
          color: hsl(211 100% 43%);
          font-weight: 600;
        }

        /* Prevent whitespace issues when copying */
        .json-date,
        .json-string,
        .json-url,
        .json-reference,
        .json-uuid,
        .json-number,
        .json-boolean,
        .json-null {
          white-space: normal;
          display: inline-block;
          word-break: break-word;
          overflow-wrap: break-word;
          max-width: 100%;
        }

        .json-date > span,
        .json-string > span,
        .json-url > span,
        .json-reference > span,
        .json-uuid > span {
          display: inline;
          word-break: break-word;
          overflow-wrap: break-word;
        }

        /* Ensure value text doesn't include leading/trailing whitespace */
        .json-date > span[style*="user-select: text"],
        .json-string > span[style*="user-select: text"],
        .json-url > span[style*="user-select: text"],
        .json-reference > span[style*="user-select: text"],
        .json-uuid > span[style*="user-select: text"] {
          white-space: pre-wrap;
          word-break: break-word;
          overflow-wrap: break-word;
        }

        /* Dates and timestamps - Purple/Magenta */
        .json-date {
          color: hsl(282 87% 51%);
          font-weight: 500;
        }

        /* URLs and URIs - Clickable links with cyan color */
        .json-url {
          color: hsl(188 97% 38%);
          text-decoration: none;
          cursor: pointer;
          transition: all 0.15s ease;
        }

        a.json-url {
          text-decoration: underline;
          text-decoration-style: dotted;
          text-underline-offset: 2px;
        }

        a.json-url:hover {
          color: hsl(188 97% 28%);
          text-decoration-style: solid;
          background-color: hsla(188 97% 38% / 0.1);
          border-radius: 2px;
        }

        /* FHIR references - Teal, clickable */
        a.json-reference {
          color: hsl(166 76% 37%);
          text-decoration: none;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s ease;
          border-bottom: 1px dashed currentColor;
        }

        a.json-reference:hover {
          color: hsl(166 76% 27%);
          background-color: hsla(166 76% 37% / 0.1);
          border-bottom-style: solid;
          border-radius: 2px;
        }

        /* UUIDs - Muted gray */
        .json-uuid {
          color: hsl(215 14% 53%);
          font-size: 0.95em;
          user-select: all;
          cursor: text;
        }

        /* Regular strings - Green */
        .json-string {
          color: hsl(134 61% 41%);
        }

        /* Numbers - Orange */
        .json-number {
          color: hsl(31 100% 48%);
          font-weight: 500;
        }

        /* Booleans - Indigo */
        .json-boolean {
          color: hsl(243 75% 59%);
          font-weight: 600;
        }

        /* Null values - Gray italic */
        .json-null {
          color: hsl(215 14% 53%);
          font-style: italic;
          opacity: 0.8;
        }

        /* Dark mode adjustments */
        .dark .json-key {
          color: hsl(211 100% 65%);
        }

        .dark .json-date {
          color: hsl(282 87% 71%);
        }

        .dark .json-url {
          color: hsl(188 97% 58%);
        }

        .dark a.json-url:hover {
          color: hsl(188 97% 78%);
          background-color: hsla(188 97% 58% / 0.15);
        }

        .dark a.json-reference {
          color: hsl(166 76% 57%);
        }

        .dark a.json-reference:hover {
          color: hsl(166 76% 77%);
          background-color: hsla(166 76% 57% / 0.15);
        }

        .dark .json-uuid {
          color: hsl(215 14% 73%);
          user-select: all;
          cursor: text;
        }

        .dark .json-string {
          color: hsl(134 61% 61%);
        }

        .dark .json-number {
          color: hsl(31 100% 68%);
        }

        .dark .json-boolean {
          color: hsl(243 75% 79%);
        }

        .dark .json-null {
          color: hsl(215 14% 73%);
        }
      `}</style>
    </div>
  );
};

export default memo(JsonViewer);
