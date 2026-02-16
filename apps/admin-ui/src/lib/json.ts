// FHIR-specific patterns for value detection
export const detectValueType = (value: string) => {
  const patterns = {
    date: /^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?)?$/,
    url: /^(https?:\/\/|urn:)/,
    reference: /^[A-Z][a-zA-Z]+\/[a-zA-Z0-9\-\.]+$/,
    uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
  };

  if (patterns.date.test(value)) return 'date';
  if (patterns.url.test(value)) return 'url';
  if (patterns.reference.test(value)) return 'reference';
  if (patterns.uuid.test(value)) return 'uuid';
  return 'string';
};

// Highlight a single primitive value (string, number, boolean, null)
export const highlightPrimitiveValue = (value: unknown): string => {
  if (value === null || value === undefined) {
    return '<span class="json-null">null</span>';
  }

  if (typeof value === 'boolean') {
    return `<span class="json-boolean">${value}</span>`;
  }

  if (typeof value === 'number') {
    return `<span class="json-number">${value}</span>`;
  }

  if (typeof value === 'string') {
    const type = detectValueType(value);

    if (type === 'date') {
      return `<span class="json-date">${value}</span>`;
    }

    if (type === 'url') {
      if (value.startsWith('http')) {
        return `<a href="${value}" target="_blank" rel="noopener noreferrer" class="json-url" title="${value}">${value}</a>`;
      } else {
        return `<span class="json-url" title="${value}">${value}</span>`;
      }
    }

    if (type === 'reference') {
      return `<a href="/result?q=${encodeURIComponent(
        value
      )}" class="json-reference" title="View ${value}">${value}</a>`;
    }

    if (type === 'uuid') {
      return `<span class="json-uuid">${value}</span>`;
    }

    return `<span class="json-string">${value}</span>`;
  }

  return String(value);
};

// FHIR-aware JSON syntax highlighting with clickable links
export const highlightJson = (jsonString: string): string => {
  let result = jsonString;

  const patterns = {
    date: /^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?)?$/,
    url: /^(https?:\/\/|urn:)/,
    reference: /^[A-Z][a-zA-Z]+\/[a-zA-Z0-9\-\.]+$/,
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
        const escapedStringValue = stringValue.replace(/"/g, '&quot;');

        // Check for FHIR-specific patterns
        if (patterns.date.test(stringValue)) {
          highlightedValue = `<span class="json-date"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
        } else if (patterns.url.test(stringValue)) {
          // Make URLs clickable
          if (stringValue.startsWith('http')) {
            highlightedValue = `<a href="${stringValue}" target="_blank" rel="noopener noreferrer" class="json-url" title="${stringValue}"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></a>`;
          } else {
            highlightedValue = `<span class="json-url" title="${stringValue}"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
          }
        } else if (patterns.reference.test(stringValue)) {
          // Make FHIR references clickable (navigate to result page with query)
          highlightedValue = `<a href="/result?q=${encodeURIComponent(
            stringValue
          )}" class="json-reference" title="View ${stringValue}"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></a>`;
        } else if (patterns.uuid.test(stringValue)) {
          highlightedValue = `<span class="json-uuid"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
        } else {
          highlightedValue = `<span class="json-string"><span class="json-quote" style="user-select: none;">&quot;</span><span style="user-select: text;">${escapedStringValue}</span><span class="json-quote" style="user-select: none;">&quot;</span></span>`;
        }
      } else if (value === 'true' || value === 'false') {
        highlightedValue = `<span class="json-boolean">${value}</span>`;
      } else if (value === 'null') {
        highlightedValue = `<span class="json-null">${value}</span>`;
      } else if (!isNaN(parseFloat(value)) && value !== '[' && value !== '{') {
        highlightedValue = `<span class="json-number">${value}</span>`;
      }

      return `<span class="json-key">${key}</span>: ${highlightedValue}`;
    }
  );

  return result;
};

export const formatFileSize = (bytes: number): string => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};
