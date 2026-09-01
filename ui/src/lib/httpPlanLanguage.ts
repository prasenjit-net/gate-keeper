import type * as Monaco from "monaco-editor";

export const HTTP_PLAN_LANGUAGE_ID = "http-plan";

const HTTP_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const COMMON_HEADERS = [
  "Accept",
  "Authorization",
  "Content-Type",
  "User-Agent",
  "X-Request-Id",
];

const CONTENT_TYPES = ["application/json", "application/xml", "text/plain", "text/html"];

// [label, detail, insertText] — kept as tuples so the table stays compact.
const SCRIPT_COMPLETIONS: ReadonlyArray<readonly [string, string, string]> = [
  ["client.test", "Define an assertion test", 'client.test("${1:name}", () => {\n  ${2}\n});'],
  ["client.assert", "Assert a condition", 'client.assert(${1:condition}, "${2:message}");'],
  ["client.log", "Add a script log entry", "client.log(${1:value});"],
  ["client.variables.get", "Read request, global, or file variable", 'client.variables.get("${1:name}")'],
  ["client.variables.set", "Set a request variable", 'client.variables.set("${1:name}", ${2:value});'],
  ["client.variables.file.get", "Read a file-level @ variable", 'client.variables.file.get("${1:name}")'],
  ["request.url", "Resolved request URL", "request.url"],
  ["request.method", "Request method", "request.method"],
  ["response.status", "HTTP response status", "response.status"],
  ["response.body", "Parsed JSON body or text body", "response.body"],
  ["response.bodyText", "Raw response body text", "response.bodyText"],
  ["console.log", "Add a console log entry", "console.log(${1:value});"],
];

export interface CompletionSpec {
  label: string;
  detail: string;
  insertText: string;
  kind: "keyword" | "field" | "variable" | "snippet" | "function";
}

let monacoRegistered = false;

export function registerHttpPlanLanguage(monaco: typeof Monaco) {
  if (monacoRegistered) return;
  monacoRegistered = true;

  if (!monaco.languages.getLanguages().some((language) => language.id === HTTP_PLAN_LANGUAGE_ID)) {
    monaco.languages.register({
      id: HTTP_PLAN_LANGUAGE_ID,
      extensions: [".http"],
      aliases: ["Gate Keeper HTTP", "HTTP Plan"],
    });
  }

  monaco.languages.setMonarchTokensProvider(HTTP_PLAN_LANGUAGE_ID, {
    ignoreCase: true,
    tokenizer: {
      root: [
        [/^###.*$/, "keyword"],
        [/^@[A-Za-z0-9_.-]+\s*=.*$/, "variable"],
        [new RegExp(`\\b(${HTTP_METHODS.join("|")})\\b`), "type.identifier"],
        [/{{[^}]+}}/, "variable.predefined"],
        [/[<>]\s*{%/, "string.escape"],
        [/%}/, "string.escape"],
        [/^#.*$/, "comment"],
        [/^\/\/.*$/, "comment"],
        [/^[A-Za-z0-9-]+(?=:)/, "attribute.name"],
      ],
    },
  });

  monaco.languages.registerCompletionItemProvider(HTTP_PLAN_LANGUAGE_ID, {
    triggerCharacters: [".", "{", "@", '"'],
    provideCompletionItems(model, position) {
      const specs = httpPlanCompletions(model.getValue(), position.lineNumber, position.column);
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      return {
        suggestions: specs.map((spec) => ({
          label: spec.label,
          detail: spec.detail,
          insertText: spec.insertText,
          kind: completionKind(monaco, spec.kind),
          insertTextRules: spec.kind === "snippet"
            ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
            : undefined,
          range,
        })),
      };
    },
  });

  const typescript = (
    monaco.languages as typeof monaco.languages & {
      typescript?: {
        javascriptDefaults?: {
          addExtraLib(source: string, filePath?: string): void;
        };
      };
    }
  ).typescript;
  typescript?.javascriptDefaults?.addExtraLib(
    scriptGlobalsDeclaration(),
    "gatekeeper-http-script.d.ts",
  );
}

// Token rules and editor colors are shared between both themes; only the hex
// values differ, so each theme just supplies value lists in these orders.
const THEME_TOKENS: ReadonlyArray<readonly [token: string, fontStyle?: string]> = [
  ["keyword", "bold"],
  ["variable"],
  ["variable.predefined"],
  ["attribute.name"],
  ["string"],
  ["comment", "italic"],
];

const THEME_COLOR_KEYS = [
  "editor.background",
  "editor.foreground",
  "editorLineNumber.foreground",
  "editorCursor.foreground",
  "editor.selectionBackground",
  "editor.lineHighlightBackground",
] as const;

const HTTP_PLAN_THEMES: ReadonlyArray<{
  name: string;
  base: "vs" | "vs-dark";
  foregrounds: readonly string[];
  colors: readonly string[];
}> = [
  {
    name: "gate-keeper-light",
    base: "vs",
    foregrounds: ["0d7d59", "3f63e0", "9c7215", "5d5a70", "17825c", "8f8ca1"],
    colors: ["#ffffff", "#2b2938", "#8f8ca1", "#0d7d59", "#d8f5e8", "#f1f0f3"],
  },
  {
    name: "gate-keeper-dark",
    base: "vs-dark",
    foregrounds: ["28e99f", "8ba4ff", "dfb35c", "a5a1b8", "5fce97", "757088"],
    colors: ["#1d1b26", "#eae8f2", "#757088", "#28e99f", "#164836", "#272432"],
  },
];

export function defineHttpPlanThemes(monaco: typeof Monaco) {
  for (const theme of HTTP_PLAN_THEMES) {
    const rules = THEME_TOKENS.map(([token, fontStyle], index) => {
      const rule: Monaco.editor.ITokenThemeRule = {
        token,
        foreground: theme.foregrounds[index],
      };
      if (fontStyle) rule.fontStyle = fontStyle;
      return rule;
    });
    monaco.editor.defineTheme(theme.name, {
      base: theme.base,
      inherit: true,
      rules,
      colors: Object.fromEntries(
        THEME_COLOR_KEYS.map((key, index) => [key, theme.colors[index]]),
      ),
    });
  }
}

export function extractHttpPlanVariables(content: string): string[] {
  const variables = new Set<string>();
  for (const line of content.split(/\r?\n/)) {
    const match = line.match(/^@([A-Za-z0-9_.-]+)\s*=/);
    if (match) variables.add(match[1]);
  }
  return [...variables].sort((a, b) => a.localeCompare(b));
}

export function isInsideScriptBlock(content: string, lineNumber: number): boolean {
  const lines = content.split(/\r?\n/);
  let inside = false;
  for (let index = 0; index < Math.min(lineNumber, lines.length); index += 1) {
    const line = lines[index];
    if (/[<>]\s*{%/.test(line)) inside = true;
    if (/%}/.test(line)) inside = false;
  }
  return inside;
}

export function httpPlanCompletions(content: string, lineNumber: number, column: number): CompletionSpec[] {
  const currentLine = content.split(/\r?\n/)[lineNumber - 1] ?? "";
  const prefix = currentLine.slice(0, Math.max(0, column - 1));
  const variables = extractHttpPlanVariables(content);

  if (prefix.endsWith("{{") || /{{[\w.-]*$/.test(prefix)) {
    return variables.map((variable) => ({
      label: variable,
      detail: "File variable",
      insertText: prefix.endsWith("{{") ? `${variable}}}` : variable,
      kind: "variable",
    }));
  }

  if (isInsideScriptBlock(content, lineNumber)) {
    return scriptCompletions(variables);
  }

  const suggestions: CompletionSpec[] = [
    ...HTTP_METHODS.map((method) => ({
      label: method,
      detail: "HTTP method",
      insertText: `${method} \${1:https://example.com}`,
      kind: "snippet" as const,
    })),
    ...COMMON_HEADERS.map((header) => ({
      label: header,
      detail: "HTTP header",
      insertText: `${header}: \${1:value}`,
      kind: "snippet" as const,
    })),
    ...CONTENT_TYPES.map((contentType) => ({
      label: contentType,
      detail: "Content type",
      insertText: contentType,
      kind: "field" as const,
    })),
    {
      label: "response assertion block",
      detail: "Response handler script block",
      insertText:
        '> {%\nclient.test("${1:status}", () => {\n  client.assert(response.status === ${2:200});\n});\n%}',
      kind: "snippet",
    },
    {
      label: "pre-request script block",
      detail: "Pre-request script block",
      insertText: '< {%\nrequest.variables.set("${1:name}", "${2:value}");\n%}',
      kind: "snippet",
    },
    {
      label: "POST JSON request",
      detail: "HTTP request with JSON body",
      insertText:
        'POST ${1:https://example.com}\nContent-Type: application/json\nAccept: application/json\n\n{\n  "${2:key}": "${3:value}"\n}',
      kind: "snippet",
    },
  ];

  return suggestions;
}

function scriptCompletions(variables: string[]): CompletionSpec[] {
  return [
    ...SCRIPT_COMPLETIONS.map(([label, detail, insertText]) => ({
      label,
      detail,
      insertText,
      kind: "snippet" as const,
    })),
    ...variables.map((variable) => ({
      label: `{{${variable}}}`,
      detail: "File variable interpolation",
      insertText: `{{${variable}}}`,
      kind: "variable" as const,
    })),
  ];
}

function completionKind(monaco: typeof Monaco, kind: CompletionSpec["kind"]) {
  switch (kind) {
    case "keyword":
      return monaco.languages.CompletionItemKind.Keyword;
    case "field":
      return monaco.languages.CompletionItemKind.Field;
    case "variable":
      return monaco.languages.CompletionItemKind.Variable;
    case "function":
      return monaco.languages.CompletionItemKind.Function;
    case "snippet":
      return monaco.languages.CompletionItemKind.Snippet;
  }
}

function scriptGlobalsDeclaration(): string {
  return `
declare const client: {
  test(name: string, fn: () => void): void;
  assert(condition: boolean, message?: string): void;
  log(...values: unknown[]): void;
  variables: {
    get(name: string): unknown;
    set(name: string, value: unknown): void;
    file: { get(name: string): unknown };
    global: { get(name: string): unknown; set(name: string, value: unknown): void };
  };
};
declare const request: {
  method: string;
  url: string;
  headers: Record<string, string>;
  body?: string | null;
  variables: { get(name: string): unknown; set(name: string, value: unknown): void };
};
declare const response: {
  status: number;
  headers: Record<string, string>;
  contentType: string;
  body: unknown;
  bodyText: string;
  durationMs: number;
};
`;
}
